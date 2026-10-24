import { translateCommandError } from "$lib/command-errors";
import { invoke } from "@tauri-apps/api/core";
import { toast } from "svelte-sonner";

import {
  clone_smart_view,
  starter_smart_views,
  type SmartView,
} from "./type/smart-view";

/**
 * The saved smart views. Changes apply immediately and roll back when the
 * backend rejects them.
 */
export class SmartViewStore {
  views = $state<SmartView[]>([]);
  loaded = $state(false);
  private saving: Promise<void> = Promise.resolve();

  async load(): Promise<void> {
    try {
      const views = await invoke<SmartView[] | null>("get_smart_views");
      if (views === null) {
        // First use: offer starter views and keep them like any other.
        this.views = starter_smart_views();
        await this.persist(this.views, []);
      } else {
        this.views = views;
      }
    } catch (error) {
      toast.error(translateCommandError(error));
    } finally {
      this.loaded = true;
    }
  }

  /** Resolves once every change so far has been saved or rolled back. */
  settled(): Promise<void> {
    return this.saving;
  }

  find(id: string): SmartView | undefined {
    return this.views.find((view) => view.id === id);
  }

  save(view: SmartView): Promise<boolean> {
    const previous = this.views;
    const index = previous.findIndex((candidate) => candidate.id === view.id);
    const next =
      index < 0
        ? [...previous, clone_smart_view(view)]
        : previous.map((candidate, i) =>
            i === index ? clone_smart_view(view) : candidate,
          );
    return this.replace(next, previous);
  }

  remove(id: string): Promise<boolean> {
    const previous = this.views;
    return this.replace(
      previous.filter((view) => view.id !== id),
      previous,
    );
  }

  /** Moves a view to `index` in the list. */
  move(id: string, index: number): Promise<boolean> {
    const previous = this.views;
    const view = previous.find((candidate) => candidate.id === id);
    if (!view) {
      return Promise.resolve(false);
    }
    const next = previous.filter((candidate) => candidate !== view);
    next.splice(Math.max(0, Math.min(index, next.length)), 0, view);
    return this.replace(next, previous);
  }

  private replace(next: SmartView[], previous: SmartView[]): Promise<boolean> {
    this.views = next;
    return this.persist(next, previous);
  }

  // Saves run in order so a slow earlier save cannot overwrite a later one.
  private persist(next: SmartView[], previous: SmartView[]): Promise<boolean> {
    const result = this.saving.then(() =>
      invoke("set_smart_views", { views: $state.snapshot(next) }).then(
        () => true,
        (error) => {
          if (this.views === next) {
            this.views = previous;
          }
          toast.error(translateCommandError(error));
          return false;
        },
      ),
    );
    this.saving = result.then(() => undefined);
    return result;
  }
}
