import { translateCommandError } from "$lib/command-errors";
import { m } from "$lib/paraglide/messages.js";
import { applyLanguagePreference, type LanguagePreference } from "$lib/i18n";
import { logger } from "$lib/logger";
import { invoke } from "@tauri-apps/api/core";
import { emit } from "@tauri-apps/api/event";
import { save, open } from "@tauri-apps/plugin-dialog";
import { writeTextFile, readTextFile } from "@tauri-apps/plugin-fs";
import { toast } from "svelte-sonner";
import {
  isPermissionGranted,
  requestPermission,
} from "@tauri-apps/plugin-notification";
import {
  type Task,
  serialize_task,
  deserialize_task,
  type TaskTemplateSerialized,
} from "./type/task.svelte";
import { parse_import_summary, type ImportSummary } from "./utils/import-data";
import { addMission } from "./utils/mission.svelte";
import type { BoardSummary } from "./board-base.svelte";
import { TaskBoardStore } from "./board-tasks.svelte";

export type { AllTaskItem, BoardRole, BoardSummary } from "./board-base.svelte";

export interface ImportCandidate {
  json_data: string;
  summary: ImportSummary;
  board_name: string;
}

// ============ Board Store (class-based for Svelte 5 rune compatibility) ============

export class BoardStore extends TaskBoardStore {
  notify_enabled = $state(false);
  task_expand_mode = $state(true);
  notification_setting_updating = $state(false);
  private expired_task_checker: ReturnType<typeof setInterval> | undefined;

  async init() {
    await this.get_boards();
    this.update_labels();
    this.get_columns();
    this.get_task_templates();

    // Load settings
    await this.load_settings();
    await this.show_recovery_messages();
    this.start_iroh_background_sync();

    // Check notification permission and start expired task checker (only if enabled)
    if (this.notify_enabled) {
      const permission_granted = await this.enable_expired_task_notifications();
      if (!permission_granted) {
        await this.set_notify_enabled(false);
        toast.warning(m.settings_notify_permission_disabled());
      }
    }
  }

  dispose() {
    this.disposed = true;
    this.stop_expired_task_checker();
    this.stop_iroh_background_sync();
    // Let pending user writes finish; only recurring background work is stopped.
  }

  private async load_settings() {
    try {
      const settings = await invoke<{
        notify_enabled: boolean;
        task_expand_mode?: boolean;
        iroh_network?: { enabled: boolean };
        language?: LanguagePreference;
      }>("get_settings");
      this.notify_enabled = settings.notify_enabled;
      this.task_expand_mode = settings.task_expand_mode ?? true;
      this.iroh_enabled = settings.iroh_network?.enabled ?? false;
      applyLanguagePreference(settings.language);
    } catch (e) {
      logger.warn("settings.load.failed", e);
    }
  }

  async set_notify_enabled(enabled: boolean) {
    if (this.notification_setting_updating) {
      return;
    }
    this.notification_setting_updating = true;

    try {
      if (enabled && !(await this.request_notification_permission())) {
        toast.error(m.settings_notify_permission_error());
        return;
      }

      const saved = await addMission(async () => {
        try {
          await invoke<void>("set_notify_enabled", { enabled });
          return true;
        } catch (e: unknown) {
          logger.error("settings.notification_save.failed", e);
          toast.error(m.settings_notify_save_error(), {
            description: translateCommandError(e),
          });
          return false;
        }
      });
      if (!saved) {
        return;
      }

      this.notify_enabled = enabled;
      if (enabled) {
        this.start_expired_task_checker();
      } else {
        this.stop_expired_task_checker();
      }
    } finally {
      this.notification_setting_updating = false;
    }
  }

  async set_task_expand_mode(expanded: boolean) {
    const previous = this.task_expand_mode;
    this.task_expand_mode = expanded;
    // Switching the board-wide mode resets individually expanded tasks.
    this.task_expansion.clear();
    const saved = await addMission(async () => {
      try {
        await invoke<void>("set_task_expand_mode", { expanded });
        return true;
      } catch (error) {
        logger.error("settings.task_expand_save.failed", error);
        toast.error(translateCommandError(error));
        return false;
      }
    });
    if (!saved && this.task_expand_mode === expanded) {
      this.task_expand_mode = previous;
    }
  }

  async set_language(preference: LanguagePreference): Promise<boolean> {
    const saved = await addMission(async () => {
      try {
        await invoke("set_language", { language: preference });
        return true;
      } catch (error) {
        logger.error("settings.language_save.failed", error);
        toast.error(translateCommandError(error));
        return false;
      }
    });
    if (saved) {
      applyLanguagePreference(preference);
      try {
        await emit("cardbe:language-changed", preference);
      } catch (error) {
        logger.warn("settings.language_broadcast.failed", error);
      }
    }
    return saved;
  }

  private async request_notification_permission(): Promise<boolean> {
    try {
      let permission_granted = await isPermissionGranted();
      if (!permission_granted) {
        const permission = await requestPermission();
        permission_granted = permission === "granted";
      }
      return permission_granted;
    } catch (e: unknown) {
      logger.warn("notifications.permission_request.failed", e);
      return false;
    }
  }

  private async enable_expired_task_notifications(): Promise<boolean> {
    const permission_granted = await this.request_notification_permission();
    if (permission_granted && this.notify_enabled) {
      this.start_expired_task_checker();
    }
    return permission_granted;
  }

  private start_expired_task_checker() {
    if (this.disposed || this.expired_task_checker !== undefined) {
      return;
    }

    const check_expired_tasks = async () => {
      if (!this.notify_enabled) {
        return;
      }
      try {
        await invoke("check_expired_tasks", {
          titleTemplate: m.notification_expired_title({ board: "{board}" }),
          bodyTemplate: m.notification_expired_body({ task: "{task}" }),
        });
      } catch (e: unknown) {
        logger.warn("notifications.expired_task_check.failed", e);
      }
    };

    void check_expired_tasks();
    // Check every minute for expired tasks
    this.expired_task_checker = setInterval(() => {
      void check_expired_tasks();
    }, 60000);
  }

  private stop_expired_task_checker() {
    if (this.expired_task_checker === undefined) {
      return;
    }
    clearInterval(this.expired_task_checker);
    this.expired_task_checker = undefined;
  }

  // ============ Task Templates ============

  save_task_template(task_id: string, name: string): Promise<boolean> {
    const expectedBoardId = this.active_board_id;
    const generation = this.board_generation;
    if (expectedBoardId === null) {
      return Promise.resolve(false);
    }
    const task_id_resolution = this.capture_task_id(task_id);
    return addMission(async () =>
      invoke<TaskTemplateSerialized>("save_task_template", {
        taskId: await task_id_resolution.id,
        name,
        expectedBoardId,
      }),
    )
      .then((template) => {
        if (generation !== this.board_generation) {
          return false;
        }
        const task = deserialize_task(template.task);
        task.id = "";
        this.templates = [
          ...this.templates,
          { id: template.id, name: template.name, task },
        ];
        this.show_success_with_undo(m.task_template_saved());
        return true;
      })
      .catch((e: unknown) => {
        logger.error("template.create.failed", e);
        if (generation === this.board_generation) {
          toast.error(translateCommandError(e));
        }
        return false;
      });
  }

  update_task_template(
    template_id: number,
    name: string,
    task: Task,
  ): Promise<boolean> {
    const expectedBoardId = this.active_board_id;
    const generation = this.board_generation;
    if (expectedBoardId === null) {
      return Promise.resolve(false);
    }
    return addMission(() =>
      invoke<TaskTemplateSerialized>("update_task_template", {
        templateId: template_id,
        name,
        task: serialize_task(task),
        expectedBoardId,
      }),
    )
      .then((template) => {
        if (generation !== this.board_generation) {
          return false;
        }
        const updated_task = deserialize_task(template.task);
        updated_task.id = "";
        this.templates = this.templates.map((candidate) =>
          candidate.id === template.id
            ? { id: template.id, name: template.name, task: updated_task }
            : candidate,
        );
        this.show_success_with_undo(m.task_template_updated());
        return true;
      })
      .catch((e: unknown) => {
        logger.error("template.update.failed", e);
        if (generation === this.board_generation) {
          toast.error(translateCommandError(e));
        }
        return false;
      });
  }

  async copy_task_template_to_board(
    template_id: number,
    target_board_id: number,
  ): Promise<boolean> {
    const expectedBoardId = this.active_board_id;
    try {
      await addMission(() =>
        invoke<void>("copy_task_template_to_board", {
          expectedBoardId,
          templateId: template_id,
          targetBoardId: target_board_id,
        }),
      );
      await this.get_boards();
      toast.success(m.template_copied_board());
      return true;
    } catch (e) {
      logger.error("template.copy_to_board.failed", e);
      toast.error(translateCommandError(e));
      return false;
    }
  }

  delete_task_template(template_id: number): Promise<boolean> {
    const expectedBoardId = this.active_board_id;
    const generation = this.board_generation;
    if (expectedBoardId === null) {
      return Promise.resolve(false);
    }
    return addMission(() =>
      invoke<void>("delete_task_template", {
        templateId: template_id,
        expectedBoardId,
      }),
    )
      .then(() => {
        if (generation !== this.board_generation) {
          return false;
        }
        this.templates = this.templates.filter(
          (template) => template.id !== template_id,
        );
        this.show_success_with_undo(m.task_template_deleted());
        return true;
      })
      .catch((e: unknown) => {
        logger.error("template.delete.failed", e);
        if (generation === this.board_generation) {
          toast.error(translateCommandError(e));
        }
        return false;
      });
  }

  private async show_recovery_messages() {
    try {
      const messages = await invoke<{ code: "STORAGE_RECOVERED" }[]>(
        "take_recovery_messages",
      );
      if (messages.length > 0) {
        toast.warning(m.backup_recovery(), {
          description: m.backup_recovery_description(),
          duration: 15000,
        });
      }
    } catch (e) {
      logger.warn("storage.recovery_messages_load.failed", e);
    }
  }

  // ============ Import/Export ============

  private timestamped_filename(prefix: string): string {
    const now = new Date();
    const part = (value: number) => String(value).padStart(2, "0");
    return `${prefix}-${now.getFullYear()}-${part(now.getMonth() + 1)}-${part(now.getDate())}_${part(now.getHours())}-${part(now.getMinutes())}-${part(now.getSeconds())}.json`;
  }

  async export_to_file() {
    try {
      const file_path = await save({
        filters: [{ name: "JSON", extensions: ["json"] }],
        defaultPath: this.timestamped_filename("cardbe-board-export"),
      });

      if (!file_path) {
        return;
      }

      const expectedBoardId = this.active_board_id;
      if (expectedBoardId === null) {
        return;
      }
      const data = await invoke<string>("export_data", { expectedBoardId });
      await writeTextFile(file_path, data);
      toast.success(m.board_exported());
    } catch (e) {
      logger.error("board.export.failed", e);
      toast.error(translateCommandError(e));
    }
  }

  async export_all_boards_to_file() {
    try {
      const file_path = await save({
        filters: [{ name: m.backup_file_type(), extensions: ["json"] }],
        defaultPath: this.timestamped_filename("cardbe-everything-backup"),
      });
      if (!file_path) {
        return;
      }
      await writeTextFile(file_path, await invoke<string>("export_all_boards"));
      toast.success(m.backup_created());
    } catch (e) {
      logger.error("backup.export.failed", e);
      toast.error(translateCommandError(e));
    }
  }

  async import_all_boards_from_file(
    confirm_restore = true,
    before_restore?: () => Promise<void>,
  ): Promise<boolean> {
    let share_links_revoked = false;
    try {
      const file_path = await open({
        filters: [{ name: "JSON", extensions: ["json"] }],
        multiple: false,
      });
      if (!file_path) {
        return false;
      }
      const jsonData = await readTextFile(file_path as string);
      await invoke("validate_all_boards_backup", { jsonData });
      if (confirm_restore && !window.confirm(m.backup_restore_confirm())) {
        return false;
      }
      await before_restore?.();
      share_links_revoked = before_restore !== undefined;
      await invoke("import_all_boards", { jsonData });
      await this.get_boards();
      this.board_generation++;
      this.column_fetch_finish = false;
      this.archives_loaded = false;
      this.get_columns();
      this.update_labels();
      this.get_task_templates();
      await this.load_settings();
      toast.success(m.backup_restored());
      return true;
    } catch (e) {
      logger.error("backup.restore.failed", e);
      toast.error(
        share_links_revoked
          ? m.backup_restore_revoked_error()
          : translateCommandError(e),
        {
          description: share_links_revoked
            ? translateCommandError(e)
            : undefined,
        },
      );
      return false;
    }
  }

  async prepare_import_from_file(): Promise<ImportCandidate | null> {
    try {
      const file_path = await open({
        filters: [{ name: "JSON", extensions: ["json"] }],
        multiple: false,
      });

      if (!file_path) {
        return null;
      }

      const data = await readTextFile(file_path as string);
      const filename =
        String(file_path).split(/[\\/]/).pop() ?? m.ui_imported_board();
      const board_name =
        filename.replace(/\.json$/i, "").trim() || m.ui_imported_board();
      return {
        json_data: data,
        summary: parse_import_summary(data),
        board_name,
      };
    } catch (e) {
      logger.error("board.import_file_read.failed", e);
      toast.error(m.backup_import_read_error());
      return null;
    }
  }

  async import_board_as_new(candidate: ImportCandidate): Promise<boolean> {
    try {
      const created = await invoke<BoardSummary>("import_board_as_new", {
        jsonData: candidate.json_data,
        name: candidate.board_name,
      });
      await this.get_boards();
      this.active_board_id = created.id;
      this.board_generation++;
      this.column_fetch_finish = false;
      this.archives_loaded = false;
      this.get_columns();
      this.update_labels();
      this.get_task_templates();
      toast.success(m.board_imported_named({ name: created.name }));
      return true;
    } catch (e) {
      logger.error("board.import_as_new.failed", e);
      toast.error(translateCommandError(e));
      return false;
    }
  }

  import_data(json_data: string): Promise<boolean> {
    const expectedBoardId = this.active_board_id;
    const generation = this.board_generation;
    if (expectedBoardId === null) {
      return Promise.resolve(false);
    }
    return addMission(() =>
      invoke<string>("import_data", { jsonData: json_data, expectedBoardId }),
    )
      .then((snapshot_path) => {
        if (generation !== this.board_generation) {
          return false;
        }
        this.update_labels();
        this.get_columns();
        if (this.archives_loaded) {
          this.get_archives();
        }
        this.get_task_templates();
        this.can_undo = true;
        toast.success(m.backup_imported(), {
          id: "data-import-success",
          description: m.backup_pre_import_path({ path: snapshot_path }),
          duration: 10000,
          action: this.can_undo
            ? {
                label: m.common_undo(),
                onClick: () => void this.undo(),
              }
            : undefined,
        });
        return true;
      })
      .catch((e: unknown) => {
        logger.error("board.import_data.failed", e);
        if (generation === this.board_generation) {
          toast.error(translateCommandError(e));
        }
        return false;
      });
  }
}

// Singleton instance
export const board = new BoardStore();

if (import.meta.hot) {
  import.meta.hot.dispose(() => board.dispose());
}
