import { translateCommandError } from "$lib/command-errors";
import { m } from "$lib/paraglide/messages.js";
import { logger } from "$lib/logger";
import { invoke } from "@tauri-apps/api/core";
import { toast } from "svelte-sonner";
import {
  get_column_id,
  set_column_id,
  type Column,
  type ColumnSort,
} from "./type/column.svelte";
import {
  type Task,
  clone_task,
  duplicate_task as create_task_duplicate,
  get_task_id,
  set_task_id,
  serialize_task,
} from "./type/task.svelte";
import { addMission } from "./utils/mission.svelte";
import { IrohBoardStore } from "./board-iroh.svelte";
import { open_floating_task } from "$lib/floating-tasks";
import {
  appended_layout,
  tab_label,
  type FlowData,
  type FlowTab,
} from "./components/flow/flow";

// Keep drag-and-drop responsive by waiting until the user has finished a
// short sequence of moves before persisting it to disk.
const DRAG_PERSIST_DEBOUNCE_MS = 300;

interface CapturedTaskId {
  was_pending: boolean;
  id: Promise<number>;
}

// Optimistic task and column edits, including drag-and-drop persistence
// and temporary IDs for tasks whose creation is still queued.
export abstract class TaskBoardStore extends IrohBoardStore {
  private pending_task_creations = new Map<string, Promise<string>>();
  // Dialogs and delayed callbacks can retain a temporary ID after creation
  // has completed, so keep a session-scoped alias to its persisted ID.
  private resolved_task_ids = new Map<string, string>();

  private find_task_position(task_id: string) {
    const current_task_id = this.resolve_task_alias(task_id);
    for (let column_idx = 0; column_idx < this.columns.length; column_idx++) {
      const task_idx = this.columns[column_idx].tasks.findIndex(
        (task) => task.id === current_task_id,
      );
      if (task_idx !== -1) {
        return { column_idx, task_idx };
      }
    }
    return undefined;
  }

  private resolve_task_alias(task_id: string): string {
    return this.resolved_task_ids.get(task_id) ?? task_id;
  }

  protected capture_task_id(task_id: string): CapturedTaskId {
    const resolved_task_id = this.resolve_task_alias(task_id);
    if (resolved_task_id !== task_id) {
      return {
        was_pending: false,
        id: Promise.resolve(get_task_id(resolved_task_id)),
      };
    }
    const pending_creation = this.pending_task_creations.get(task_id);
    return {
      was_pending: pending_creation !== undefined,
      id: (pending_creation ?? Promise.resolve(task_id)).then(
        (persisted_task_id) => get_task_id(persisted_task_id),
      ),
    };
  }

  private cancel_task_move_timer(task_id: string) {
    const timer = this.task_move_timers.get(task_id);
    if (!timer) {
      return;
    }
    clearTimeout(timer);
    this.task_move_timers.delete(task_id);
  }

  private forget_task_move_timer(timer: ReturnType<typeof setTimeout>) {
    for (const [task_id, candidate] of this.task_move_timers) {
      if (candidate === timer) {
        this.task_move_timers.delete(task_id);
      }
    }
  }

  private rekey_task_move_timer(
    pending_task_id: string,
    persisted_task_id: string,
  ) {
    const timer = this.task_move_timers.get(pending_task_id);
    if (!timer) {
      return;
    }
    this.task_move_timers.delete(pending_task_id);

    // A move made after creation resolved is newer and should win over a
    // timer that was scheduled while the task still had its temporary ID.
    if (this.task_move_timers.has(persisted_task_id)) {
      clearTimeout(timer);
      return;
    }
    this.task_move_timers.set(persisted_task_id, timer);
  }

  add_new_task(
    column_id: string,
    task: Task,
    after_task_id: string | null = null,
    onCreated?: (id: string) => void,
  ): Promise<boolean> {
    const board = this;
    const expectedBoardId = this.active_board_id;
    const generation = this.board_generation;
    if (expectedBoardId === null) {
      return Promise.resolve(false);
    }
    const new_task = clone_task(task);
    new_task.created_time ??= new Date();

    // Use a temporary ID so the new card can appear before the backend has
    // allocated its persistent ID.
    new_task.id = `pending_task_${globalThis.crypto.randomUUID()}`;
    const pending_task_id = new_task.id;
    const column = this.columns.find((candidate) => candidate.id === column_id);
    if (!column) {
      toast.error(m.column_missing());
      return Promise.resolve(false);
    }
    const after_task_idx =
      after_task_id === null
        ? -1
        : column.tasks.findIndex(
            (candidate) =>
              candidate.id === this.resolve_task_alias(after_task_id),
          );
    if (after_task_id !== null && after_task_idx === -1) {
      toast.error(m.task_original_missing());
      return Promise.resolve(false);
    }
    const after_task_resolution =
      after_task_id === null ? null : this.capture_task_id(after_task_id);
    const insertion_idx =
      after_task_idx === -1 ? column.tasks.length : after_task_idx + 1;
    column.tasks.splice(insertion_idx, 0, new_task);
    this.sort_column_tasks(column);
    this.columns = [...this.columns];
    this.refresh_labels_from_columns();

    const creation = addMission(async () =>
      invoke<number>("add_task", {
        columnId: get_column_id(column_id),
        task: serialize_task(new_task),
        afterTaskId:
          after_task_resolution === null
            ? null
            : await after_task_resolution.id,
        expectedBoardId,
      }),
    )
      .then((new_id: number) => {
        if (generation !== board.board_generation) {
          return `task_${new_id}`;
        }
        // The card may have been edited while its creation was still
        // queued, so update the current card rather than only the
        // original optimistic object.
        const position = this.find_task_position(pending_task_id);
        if (position) {
          set_task_id(
            this.columns[position.column_idx].tasks[position.task_idx],
            new_id,
          );
        }
        this.columns = [...this.columns];
        this.update_labels();
        this.show_success_with_undo(m.task_added());
        onCreated?.(`task_${new_id}`);
        const persisted_task_id = `task_${new_id}`;
        this.resolved_task_ids.set(pending_task_id, persisted_task_id);
        this.rekey_task_move_timer(pending_task_id, persisted_task_id);
        return persisted_task_id;
      })
      .catch((e: unknown) => {
        logger.error("task.create.failed", e);
        if (generation !== board.board_generation) {
          throw e;
        }
        const position = this.find_task_position(pending_task_id);
        if (position) {
          this.columns[position.column_idx].tasks.splice(position.task_idx, 1);
          this.columns = [...this.columns];
        }
        this.refresh_labels_from_columns();
        toast.error(translateCommandError(e));
        throw e;
      });
    this.pending_task_creations.set(pending_task_id, creation);

    return creation
      .then(() => true)
      .catch(() => false)
      .finally(() => this.pending_task_creations.delete(pending_task_id));
  }

  duplicate_task(task_id: string): Promise<boolean> {
    const position = this.find_task_position(task_id);
    if (!position) {
      toast.error(m.task_missing());
      return Promise.resolve(false);
    }

    const column = this.columns[position.column_idx];
    return this.add_new_task(
      column.id,
      create_task_duplicate(column.tasks[position.task_idx]),
      task_id,
    );
  }

  delete_task(task_id: string): Promise<boolean> {
    const expectedBoardId = this.active_board_id;
    const generation = this.board_generation;
    if (expectedBoardId === null) {
      return Promise.resolve(false);
    }
    const position = this.find_task_position(task_id);
    if (!position) {
      return Promise.resolve(false);
    }
    const task_id_resolution = this.capture_task_id(task_id);
    const current_task_id = this.resolve_task_alias(task_id);
    const deleted_task = clone_task(
      this.columns[position.column_idx].tasks[position.task_idx],
    );
    const column_id = this.columns[position.column_idx].id;
    this.cancel_task_move_timer(current_task_id);
    this.columns[position.column_idx].tasks.splice(position.task_idx, 1);
    this.columns = [...this.columns];
    this.refresh_labels_from_columns();

    let task_was_created = !task_id_resolution.was_pending;
    return addMission(async () => {
      const persisted_task_id = await task_id_resolution.id;
      task_was_created = true;
      set_task_id(deleted_task, persisted_task_id);
      await invoke<void>("delete_task", {
        taskId: persisted_task_id,
        expectedBoardId,
      });
    })
      .then(() => {
        if (generation !== this.board_generation) {
          return false;
        }
        this.search_tasks(this.search_query);
        this.show_success_with_undo(m.task_deleted());
        return true;
      })
      .catch((e: unknown) => {
        logger.error("task.delete.failed", e);
        if (generation !== this.board_generation) {
          return false;
        }
        if (task_was_created) {
          const column = this.columns.find(
            (candidate) => candidate.id === column_id,
          );
          if (
            column &&
            !column.tasks.some((task) => task.id === deleted_task.id)
          ) {
            column.tasks.splice(position.task_idx, 0, deleted_task);
            this.columns = [...this.columns];
            this.refresh_labels_from_columns();
          }
          toast.error(translateCommandError(e));
        }
        return false;
      });
  }

  archive_task(task_id: string): Promise<boolean> {
    const board = this;
    const expectedBoardId = this.active_board_id;
    const generation = this.board_generation;
    if (expectedBoardId === null) {
      return Promise.resolve(false);
    }
    const position = this.find_task_position(task_id);
    if (!position) {
      return Promise.resolve(false);
    }
    const task_id_resolution = this.capture_task_id(task_id);
    const current_task_id = this.resolve_task_alias(task_id);
    const archived_task = clone_task(
      this.columns[position.column_idx].tasks[position.task_idx],
    );
    const column_id = this.columns[position.column_idx].id;
    this.cancel_task_move_timer(current_task_id);
    this.columns[position.column_idx].tasks.splice(position.task_idx, 1);
    this.columns = [...this.columns];
    this.refresh_labels_from_columns();

    let task_was_created = !task_id_resolution.was_pending;
    return addMission(async () => {
      const persisted_task_id = await task_id_resolution.id;
      task_was_created = true;
      set_task_id(archived_task, persisted_task_id);
      await invoke<void>("archive_task", {
        taskId: persisted_task_id,
        expectedBoardId,
      });
    })
      .then(() => {
        if (generation !== board.board_generation) {
          return false;
        }
        board.get_columns();
        board.refresh_archives_if_loaded();
        this.show_success_with_undo(m.task_archived());
        return true;
      })
      .catch((e: unknown) => {
        logger.error("task.archive.failed", e);
        if (generation !== board.board_generation) {
          return false;
        }
        if (task_was_created) {
          const column = this.columns.find(
            (candidate) => candidate.id === column_id,
          );
          if (
            column &&
            !column.tasks.some((task) => task.id === archived_task.id)
          ) {
            column.tasks.splice(position.task_idx, 0, archived_task);
            this.columns = [...this.columns];
            this.refresh_labels_from_columns();
          }
          toast.error(translateCommandError(e));
        }
        return false;
      });
  }

  archive_all_tasks(column_id: string) {
    const board = this;
    const expectedBoardId = this.active_board_id;
    const generation = this.board_generation;
    if (expectedBoardId === null) {
      return;
    }
    const column = board.columns.find(
      (candidate) => candidate.id === column_id,
    );
    if (!column) {
      return;
    }
    const archived_tasks = column.tasks.map(clone_task);
    column.tasks = [];
    board.columns = [...board.columns];
    board.refresh_labels_from_columns();

    addMission(async () => {
      return invoke<void>("archive_all_tasks", {
        columnId: get_column_id(column_id),
        expectedBoardId,
      })
        .then(() => {
          if (generation !== board.board_generation) {
            return;
          }
          board.get_columns();
          board.refresh_archives_if_loaded();
          this.show_success_with_undo(m.task_archived_all());
        })
        .catch((e: unknown) => {
          logger.error("task.archive_all.failed", e);
          if (generation !== board.board_generation) {
            return;
          }
          const current_column = board.columns.find(
            (candidate) => candidate.id === column_id,
          );
          if (current_column) {
            current_column.tasks = archived_tasks;
            board.columns = [...board.columns];
            board.refresh_labels_from_columns();
          }
          toast.error(translateCommandError(e));
        });
    });
  }

  delete_tasks(task_ids: string[]): Promise<boolean> {
    return this.run_task_batch(
      task_ids,
      (selected) => this.remove_tasks(selected),
      (taskIds, expectedBoardId) =>
        invoke<void>("delete_tasks", { taskIds, expectedBoardId }),
      (count) => {
        this.search_tasks(this.search_query);
        this.show_success_with_undo(m.task_deleted_count({ count }));
      },
      "task.delete_batch.failed",
    );
  }

  archive_tasks(task_ids: string[]): Promise<boolean> {
    return this.run_task_batch(
      task_ids,
      (selected) => this.remove_tasks(selected),
      (taskIds, expectedBoardId) =>
        invoke<void>("archive_tasks", { taskIds, expectedBoardId }),
      (count) => {
        this.get_columns();
        this.refresh_archives_if_loaded();
        this.show_success_with_undo(m.task_archived_count({ count }));
      },
      "task.archive_batch.failed",
    );
  }

  // `task_ids` order is kept, so callers pass the order shown on the board.
  move_tasks(
    task_ids: string[],
    to_column_id: string,
    before_task_id: string | null = null,
  ): Promise<boolean> {
    if (!this.columns.some((column) => column.id === to_column_id)) {
      return Promise.resolve(false);
    }
    const before_task_resolution =
      before_task_id === null ? null : this.capture_task_id(before_task_id);
    return this.run_task_batch(
      task_ids,
      (selected) => {
        const moved = this.remove_tasks(selected);
        const to_column = this.columns.find(
          (column) => column.id === to_column_id,
        );
        if (!to_column) {
          return;
        }
        const before_idx = to_column.tasks.findIndex(
          (task) =>
            before_task_id !== null &&
            task.id === this.resolve_task_alias(before_task_id),
        );
        to_column.tasks.splice(
          before_idx === -1 ? to_column.tasks.length : before_idx,
          0,
          ...moved,
        );
        this.sort_column_tasks(to_column);
      },
      async (taskIds, expectedBoardId) =>
        invoke<void>("move_tasks", {
          taskIds,
          toColumnId: get_column_id(to_column_id),
          beforeTaskId:
            before_task_resolution === null
              ? null
              : await before_task_resolution.id,
          expectedBoardId,
        }),
      (count) => this.show_success_with_undo(m.task_moved_count({ count })),
      "task.move_batch.failed",
    );
  }

  set_tasks_pinned(task_ids: string[], pinned: boolean): Promise<boolean> {
    return this.run_task_batch(
      task_ids,
      (selected) => {
        for (const column of this.columns) {
          if (column.tasks.some((task) => selected.has(task.id))) {
            column.tasks = column.tasks.map((task) =>
              selected.has(task.id) ? { ...task, pinned } : task,
            );
            this.sort_column_tasks(column);
          }
        }
      },
      (taskIds, expectedBoardId) =>
        invoke<void>("set_tasks_pinned", { taskIds, pinned, expectedBoardId }),
      (count) =>
        this.show_success_with_undo(
          pinned
            ? m.task_pinned_count({ count })
            : m.task_unpinned_count({ count }),
        ),
      "task.pin_batch.failed",
    );
  }

  async move_tasks_to_board(
    task_ids: string[],
    target_board_id: number,
    target_column_id: number,
  ): Promise<boolean> {
    const expectedBoardId = this.active_board_id;
    const generation = this.board_generation;
    const resolutions = task_ids.map((task_id) => {
      this.cancel_task_move_timer(this.resolve_task_alias(task_id));
      return this.capture_task_id(task_id);
    });
    try {
      await addMission(async () => {
        await invoke("move_tasks_to_board", {
          expectedBoardId,
          taskIds: await Promise.all(
            resolutions.map((resolution) => resolution.id),
          ),
          targetBoardId: target_board_id,
          targetColumnId: target_column_id,
        });
        // Clear undo before the next queued edit can create new history.
        if (generation === this.board_generation) {
          this.can_undo = false;
        }
      });
      await this.get_boards();
      if (this.all_task_list_filter) {
        void this.fetch_all_task_page(true);
      }
      if (generation === this.board_generation) {
        this.get_columns();
        this.update_labels();
        this.search_tasks(this.search_query);
      }
      toast.success(m.task_moved_board_count({ count: task_ids.length }));
      return true;
    } catch (e) {
      logger.error("task.move_batch_to_board.failed", e);
      if (generation === this.board_generation) {
        this.get_columns();
      }
      toast.error(translateCommandError(e));
      return false;
    }
  }

  // Removes tasks and returns them in the order they were requested.
  private remove_tasks(task_ids: ReadonlySet<string>): Task[] {
    const removed = new Map<string, Task>();
    for (const column of this.columns) {
      for (const task of column.tasks) {
        if (task_ids.has(task.id)) {
          removed.set(task.id, task);
        }
      }
      column.tasks = column.tasks.filter((task) => !task_ids.has(task.id));
    }
    return [...task_ids].flatMap((task_id) => removed.get(task_id) ?? []);
  }

  // Applies one optimistic edit for the whole selection and persists it as a
  // single backend change, so one Undo restores every selected task.
  private run_task_batch(
    task_ids: string[],
    apply: (selected: ReadonlySet<string>) => void,
    persist: (task_ids: number[], expected_board_id: number) => Promise<void>,
    on_success: (count: number) => void,
    log_event: string,
  ): Promise<boolean> {
    const expectedBoardId = this.active_board_id;
    const generation = this.board_generation;
    if (expectedBoardId === null) {
      return Promise.resolve(false);
    }
    const selected = new Set(
      task_ids
        .map((task_id) => this.resolve_task_alias(task_id))
        .filter((task_id) => this.find_task_position(task_id)),
    );
    if (selected.size === 0) {
      return Promise.resolve(false);
    }
    const resolutions = [...selected].map((task_id) => {
      this.cancel_task_move_timer(task_id);
      return this.capture_task_id(task_id);
    });
    apply(selected);
    this.columns = [...this.columns];
    this.refresh_labels_from_columns();

    return addMission(async () => {
      // Tasks whose queued creation failed are already gone from the board.
      const persisted_ids = (
        await Promise.all(
          resolutions.map((resolution) => resolution.id.catch(() => null)),
        )
      ).filter((task_id) => task_id !== null);
      if (persisted_ids.length > 0) {
        await persist(persisted_ids, expectedBoardId);
      }
      return persisted_ids.length;
    })
      .then((count) => {
        if (generation !== this.board_generation || count === 0) {
          return false;
        }
        on_success(count);
        return true;
      })
      .catch((e: unknown) => {
        logger.error(log_event, e);
        if (generation !== this.board_generation) {
          return false;
        }
        toast.error(translateCommandError(e));
        this.get_columns();
        return false;
      });
  }

  unarchive_task(column_id: string, task_id: string) {
    const board = this;
    const expectedBoardId = this.active_board_id;
    const generation = this.board_generation;
    if (expectedBoardId === null) {
      return;
    }
    const archive_task_data = board.archives.find(
      (archive) => archive.task.id === task_id,
    );
    if (!archive_task_data) {
      return;
    }
    const archive_index = board.archives.indexOf(archive_task_data);
    const restored_task = clone_task(archive_task_data.task);
    const column = board.columns.find(
      (candidate) => candidate.id === column_id,
    );
    if (!column) {
      return;
    }
    column.tasks.push(restored_task);
    board.sort_column_tasks(column);
    board.archives.splice(archive_index, 1);
    board.columns = [...board.columns];
    board.archives = [...board.archives];
    board.refresh_labels_from_columns();

    addMission(async () => {
      return invoke<void>("unarchive_task", {
        columnId: get_column_id(column_id),
        taskId: get_task_id(task_id),
        expectedBoardId,
      })
        .then(() => {
          if (generation !== board.board_generation) {
            return;
          }
          board.get_archives();
          this.show_success_with_undo(m.task_restored());
        })
        .catch((e: unknown) => {
          logger.error("task.unarchive.failed", e);
          if (generation !== board.board_generation) {
            return;
          }
          const current_column = board.columns.find(
            (candidate) => candidate.id === column_id,
          );
          if (current_column) {
            const task_index = current_column.tasks.findIndex(
              (task) => task.id === task_id,
            );
            if (task_index !== -1) {
              current_column.tasks.splice(task_index, 1);
            }
          }
          if (!board.archives.some((archive) => archive.task.id === task_id)) {
            board.archives.splice(archive_index, 0, archive_task_data);
          }
          board.columns = [...board.columns];
          board.archives = [...board.archives];
          board.refresh_labels_from_columns();
          toast.error(translateCommandError(e));
        });
    });
  }

  unarchive_task_from_list(column_id: string, task_id: string) {
    const expectedBoardId = this.active_board_id;
    const generation = this.board_generation;
    if (expectedBoardId === null) {
      return;
    }
    addMission(() =>
      invoke<void>("unarchive_task", {
        columnId: get_column_id(column_id),
        taskId: get_task_id(task_id),
        expectedBoardId,
      }),
    )
      .then(() => {
        if (generation !== this.board_generation) {
          return;
        }
        this.get_columns();
        this.refresh_archives_if_loaded();
        this.show_success_with_undo(m.task_restored());
      })
      .catch((error: unknown) => {
        logger.error("task.unarchive_from_list.failed", error);
        if (generation === this.board_generation) {
          toast.error(translateCommandError(error));
        }
      });
  }

  update_task(task_id: string, task: Task): Promise<boolean> {
    if (!this.can_edit) {
      return Promise.resolve(false);
    }
    const board = this;
    const expectedBoardId = this.active_board_id;
    const generation = this.board_generation;
    if (expectedBoardId === null) {
      return Promise.resolve(false);
    }
    const task_id_resolution = this.capture_task_id(task_id);
    const position = this.find_task_position(task_id);
    const previous_task = position
      ? clone_task(this.columns[position.column_idx].tasks[position.task_idx])
      : undefined;
    const updated_task = clone_task(task);
    const resolved_task_id = this.resolve_task_alias(task_id);
    if (resolved_task_id !== task_id) {
      updated_task.id = resolved_task_id;
    }
    if (position) {
      this.columns[position.column_idx].tasks[position.task_idx] = updated_task;
      this.sort_column_tasks(this.columns[position.column_idx]);
      this.columns = [...this.columns];
      board.refresh_labels_from_columns();
    }

    let task_was_created = !task_id_resolution.was_pending;
    let current_task_id = this.resolve_task_alias(task_id);
    return addMission(async () => {
      const persisted_task_id = await task_id_resolution.id;
      task_was_created = true;
      current_task_id = `task_${persisted_task_id}`;
      set_task_id(updated_task, persisted_task_id);
      if (previous_task) {
        set_task_id(previous_task, persisted_task_id);
      }
      await invoke<void>("update_task", {
        taskId: persisted_task_id,
        task: serialize_task(updated_task),
        expectedBoardId,
      });
    })
      .then(() => {
        if (generation !== board.board_generation) {
          return false;
        }
        this.update_labels();
        this.search_tasks(this.search_query);
        this.show_success_with_undo(m.task_updated());
        if (this.all_task_list_filter) {
          void this.fetch_all_task_page(true);
        }
        return true;
      })
      .catch((e: unknown) => {
        logger.error("task.update.failed", e);
        if (generation !== board.board_generation) {
          return false;
        }
        const current_position = this.find_task_position(current_task_id);
        if (task_was_created && current_position && previous_task) {
          this.columns[current_position.column_idx].tasks[
            current_position.task_idx
          ] = previous_task;
          this.sort_column_tasks(this.columns[current_position.column_idx]);
          this.columns = [...this.columns];
          board.refresh_labels_from_columns();
        }
        if (task_was_created) {
          toast.error(translateCommandError(e));
        }
        return false;
      });
  }

  move_task(
    from_column_idx: number,
    from_task_idx: number,
    to_column_idx: number,
    to_task_idx: number | null,
    persist: boolean = true,
  ) {
    if (from_column_idx === to_column_idx && from_task_idx === to_task_idx) {
      return false;
    }

    const from_column = this.columns[from_column_idx];
    const to_column = this.columns[to_column_idx];
    const data = from_column?.tasks[from_task_idx];
    if (!from_column || !to_column || !data) {
      const context = {
        from_column_idx,
        from_task_idx,
        to_column_idx,
        to_task_idx,
      };
      logger.warn("board.task_move.invalid_position", undefined, context);
      return false;
    }

    from_column.tasks.splice(from_task_idx, 1);
    if (to_task_idx == null) {
      to_task_idx = to_column.tasks.length;
      to_column.tasks.push(data);
    } else {
      to_column.tasks.splice(to_task_idx, 0, data);
    }
    this.sort_column_tasks(from_column);
    if (to_column !== from_column) {
      this.sort_column_tasks(to_column);
    }
    // Trigger reactivity for Svelte 5
    this.columns = [...this.columns];

    if (!persist) {
      return true;
    }

    const final_task_idx = to_column.tasks.findIndex(
      (task) => task.id === data.id,
    );
    const before_task_id = to_column.tasks[final_task_idx + 1]?.id ?? null;
    this.persist_task_move(data.id, to_column.id, before_task_id);
    return true;
  }

  persist_task_move(
    task_id: string,
    to_column_id: string,
    before_task_id: string | null,
  ) {
    const expectedBoardId = this.active_board_id;
    const generation = this.board_generation;
    if (expectedBoardId === null) {
      return;
    }
    const task_id_resolution = this.capture_task_id(task_id);
    const before_task_id_resolution =
      before_task_id === null ? null : this.capture_task_id(before_task_id);
    const timer_task_id = this.resolve_task_alias(task_id);
    const existing_timer = this.task_move_timers.get(timer_task_id);
    if (existing_timer) {
      clearTimeout(existing_timer);
    }

    const timer = setTimeout(() => {
      this.forget_task_move_timer(timer);
      addMission(async () => {
        const persisted_task_id = await task_id_resolution.id.catch(() => null);
        if (persisted_task_id === null) {
          return false;
        }
        const persisted_before_task_id =
          before_task_id_resolution === null
            ? null
            : await before_task_id_resolution.id.catch(() => null);
        await invoke<void>("move_task", {
          taskId: persisted_task_id,
          toColumnId: get_column_id(to_column_id),
          beforeTaskId: persisted_before_task_id,
          expectedBoardId,
        });
        return true;
      })
        .then((persisted) => {
          if (persisted && generation === this.board_generation) {
            this.can_undo = true;
          }
        })
        .catch((e: unknown) => {
          logger.error("task.move.failed", e);
          if (generation !== this.board_generation) {
            return;
          }
          toast.error(translateCommandError(e));
          this.get_columns();
        });
    }, DRAG_PERSIST_DEBOUNCE_MS);

    this.task_move_timers.set(timer_task_id, timer);
  }

  async move_task_to_board(
    task_id: string,
    target_board_id: number,
    target_column_id: number,
  ): Promise<boolean> {
    const expectedBoardId = this.active_board_id;
    const generation = this.board_generation;
    const resolution = this.capture_task_id(task_id);
    this.cancel_task_move_timer(this.resolve_task_alias(task_id));
    try {
      await addMission(async () => {
        await invoke("move_task_to_board", {
          expectedBoardId,
          taskId: await resolution.id,
          targetBoardId: target_board_id,
          targetColumnId: target_column_id,
        });
        // Clear undo before the next queued edit can create new history.
        if (generation === this.board_generation) {
          this.can_undo = false;
        }
      });
      await this.get_boards();
      if (this.all_task_list_filter) {
        void this.fetch_all_task_page(true);
      }
      if (generation === this.board_generation) {
        this.get_columns();
        this.update_labels();
        this.search_tasks(this.search_query);
      }
      toast.success(m.task_moved_board());
      return true;
    } catch (e) {
      logger.error("task.move_to_board.failed", e);
      if (generation === this.board_generation) {
        this.get_columns();
      }
      toast.error(translateCommandError(e));
      return false;
    }
  }

  // ============ Column Operations ============

  add_column(name: string) {
    const board = this;
    const expectedBoardId = this.active_board_id;
    const generation = this.board_generation;
    if (expectedBoardId === null) {
      return;
    }
    addMission(async () => {
      return invoke<number>("add_column", {
        name: name,
        color: "",
        expectedBoardId,
      })
        .then((new_id) => {
          if (generation !== board.board_generation) {
            return;
          }
          board.show_success_with_undo(m.column_added());
          const column: Column = {
            id: "",
            name: name,
            color: "",
            sort_order: "custom",
            tasks: [],
          };
          set_column_id(column, new_id);
          board.columns.push(column);
          // Trigger reactivity for Svelte 5
          board.columns = [...board.columns];
        })
        .catch((e: unknown) => {
          logger.error("column.create.failed", e);
          if (generation !== board.board_generation) {
            return;
          }
          toast.error(translateCommandError(e));
        });
    });
  }

  update_column(column_id: string, name: string, sort_order?: ColumnSort) {
    const board = this;
    const expectedBoardId = this.active_board_id;
    const generation = this.board_generation;
    if (expectedBoardId === null) {
      return;
    }
    const column = board.columns.find(
      (candidate) => candidate.id === column_id,
    );
    const next_sort_order = sort_order ?? column?.sort_order ?? "custom";
    addMission(async () => {
      return invoke<void>("update_column", {
        columnId: get_column_id(column_id),
        name: name,
        color: "",
        sortOrder: next_sort_order,
        expectedBoardId,
      })
        .then(() => {
          if (generation !== board.board_generation) {
            return;
          }
          board.show_success_with_undo(m.column_updated());
          const updated_column = board.columns.find(
            (candidate) => candidate.id === column_id,
          );
          if (updated_column) {
            updated_column.name = name;
            updated_column.sort_order = next_sort_order;
            board.sort_column_tasks(updated_column);
            board.columns = [...board.columns];
          }
        })
        .catch((e: unknown) => {
          logger.error("column.update.failed", e);
          if (generation !== board.board_generation) {
            return;
          }
          toast.error(translateCommandError(e));
        });
    });
  }

  move_column(
    from_column_idx: number,
    to_column_idx: number,
    persist: boolean = true,
  ) {
    if (from_column_idx === to_column_idx) {
      return false;
    }

    const data = this.columns[from_column_idx];
    if (!data || !this.columns[to_column_idx]) {
      const context = {
        from_column_idx,
        to_column_idx,
      };
      logger.warn("board.column_move.invalid_position", undefined, context);
      return false;
    }

    this.columns.splice(from_column_idx, 1);
    this.columns.splice(to_column_idx, 0, data);
    // Trigger reactivity for Svelte 5
    this.columns = [...this.columns];

    if (!persist) {
      return true;
    }

    const before_column_id = this.columns[to_column_idx + 1]?.id ?? null;
    this.persist_column_move(data.id, before_column_id);
    return true;
  }

  persist_column_move(column_id: string, before_column_id: string | null) {
    const expectedBoardId = this.active_board_id;
    const generation = this.board_generation;
    if (expectedBoardId === null) {
      return;
    }
    const existing_timer = this.column_move_timers.get(column_id);
    if (existing_timer) {
      clearTimeout(existing_timer);
    }

    const timer = setTimeout(() => {
      this.column_move_timers.delete(column_id);
      addMission(async () => {
        return invoke<void>("move_column", {
          columnId: get_column_id(column_id),
          beforeColumnId:
            before_column_id == null ? null : get_column_id(before_column_id),
          expectedBoardId,
        })
          .then(() => {
            if (generation === this.board_generation) {
              this.can_undo = true;
            }
          })
          .catch((e: unknown) => {
            logger.error("column.move.failed", e);
            if (generation !== this.board_generation) {
              return;
            }
            toast.error(translateCommandError(e));
            this.get_columns();
          });
      });
    }, DRAG_PERSIST_DEBOUNCE_MS);

    this.column_move_timers.set(column_id, timer);
  }

  delete_column(column_id: string) {
    const expectedBoardId = this.active_board_id;
    const generation = this.board_generation;
    if (expectedBoardId === null) {
      return;
    }
    addMission(async () => {
      return invoke<void>("delete_column", {
        columnId: get_column_id(column_id),
        expectedBoardId,
      })
        .then(() => {
          if (generation !== this.board_generation) {
            return;
          }
          const index = this.columns.findIndex(
            (column) => column.id === column_id,
          );
          if (index !== -1) {
            this.columns.splice(index, 1);
          }
          this.columns = [...this.columns];
          this.show_success_with_undo(m.column_deleted());
        })
        .catch((e: unknown) => {
          logger.error("column.delete.failed", e);
          if (generation !== this.board_generation) {
            return;
          }
          toast.error(translateCommandError(e));
        });
    });
  }

  // The backend always returns at least one tab; null means the tabs are unavailable.
  async get_flow_tabs(): Promise<FlowTab[] | null> {
    const boardId = this.active_board_id;
    if (boardId === null) {
      return null;
    }
    try {
      const data = await invoke<FlowData>("get_flow", {
        expectedBoardId: boardId,
        tabId: null,
      });
      return data.tabs;
    } catch (error) {
      toast.error(translateCommandError(error));
      return null;
    }
  }

  add_task_to_flow(task_id: string, tab: FlowTab) {
    return this.add_tasks_to_flow([task_id], tab);
  }

  async add_tasks_to_flow(task_ids: string[], tab: FlowTab) {
    const boardId = this.active_board_id;
    if (boardId === null || task_ids.length === 0) {
      return;
    }
    const resolutions = task_ids.map((task_id) =>
      this.capture_task_id(task_id),
    );
    try {
      // Queue behind earlier edits so a newly created task is persisted first.
      const added = await addMission(async () => {
        const data = await invoke<FlowData>("get_flow", {
          expectedBoardId: boardId,
          tabId: tab.id,
        });
        if (data.tab_id !== tab.id) {
          throw { code: "FLOW_TAB_NOT_FOUND" };
        }
        const layout = appended_layout(
          data.layout,
          await Promise.all(resolutions.map((resolution) => resolution.id)),
        );
        if (layout) {
          await invoke("save_flow_layout", {
            expectedBoardId: boardId,
            tabId: tab.id,
            layout,
          });
        }
        return layout !== null;
      });
      const name = tab_label(tab);
      const count = task_ids.length;
      if (added) {
        toast.success(
          count === 1
            ? m.task_add_to_flow_done({ name })
            : m.task_add_to_flow_done_count({ name, count }),
        );
      } else {
        toast.info(
          count === 1
            ? m.task_add_to_flow_exists({ name })
            : m.task_add_to_flow_exists_all({ name }),
        );
      }
    } catch (error) {
      toast.error(translateCommandError(error));
    }
  }

  async float_task(task_id: string) {
    const boardId = this.active_board_id;
    if (boardId === null) {
      return;
    }
    try {
      await open_floating_task({ boardId, taskId: get_task_id(task_id) });
    } catch (error) {
      logger.error("board.task_float.failed", error);
      toast.error(translateCommandError(error));
    }
  }
}
