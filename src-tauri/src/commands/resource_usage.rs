use crate::{
    errors::CommandError,
    resource_usage::{ResourceMonitorState, ResourceUsage},
};
use tauri::State;

/// CPU and memory usage of the Cardbe process tree. Sampling runs on a
/// blocking worker; calls within the minimum interval reuse the last sample.
#[tauri::command]
pub async fn get_resource_usage(
    state: State<'_, ResourceMonitorState>,
) -> Result<ResourceUsage, CommandError> {
    let monitor = state.handle();
    tauri::async_runtime::spawn_blocking(move || monitor.sample())
        .await
        .map_err(CommandError::internal)?
        .map_err(|_| CommandError::ResourceUsageUnavailable)
}
