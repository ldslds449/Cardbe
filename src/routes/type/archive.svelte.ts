import { create_task, type Task, type TaskSerialized } from "./task.svelte";

export interface Archive {
  time: Date;
  task: Task;
}

export interface ArchiveSerialized {
  time: number;
  task: TaskSerialized;
}

export function create_archive(
  time: Date = new Date(),
  task: Task = create_task(),
): Archive {
  return {
    time: time,
    task: task,
  };
}
