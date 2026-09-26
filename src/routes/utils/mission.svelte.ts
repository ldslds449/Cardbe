import { logger } from "$lib/logger";

// Queue ensures toast notifications appear in the same order as user actions
// Backend's Mutex<AppData> ensures data consistency regardless of queue usage
let mission_queue: Promise<void> = Promise.resolve();

export function addMission<T>(mission: () => Promise<T>): Promise<T> {
  const result = mission_queue.then(mission);

  // Keep later work in the queue even when this mission fails, while still
  // returning the original rejection to the caller for proper error handling.
  mission_queue = result.then(
    () => undefined,
    (e) => {
      logger.error("mission.failed", e);
      console.error("Mission error:", e);
    },
  );
  return result;
}
