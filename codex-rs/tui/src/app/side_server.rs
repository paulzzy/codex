use super::*;
use crate::app_event::SideThreadPrepareError;
use crate::app_server_session::ThreadParamsMode;
use codex_app_server_protocol::RequestId;
use codex_app_server_protocol::ThreadInjectItemsParams;
use codex_app_server_protocol::ThreadInjectItemsResponse;

pub(super) async fn prepare_side_thread(
    request_handle: AppServerRequestHandle,
    config: Config,
    parent_thread_id: ThreadId,
    thread_params_mode: ThreadParamsMode,
    remote_cwd_override: Option<PathBuf>,
) -> std::result::Result<ThreadSessionState, SideThreadPrepareError> {
    let boundary_item = serde_json::to_value(App::side_boundary_prompt_item()).map_err(|err| {
        SideThreadPrepareError {
            session: None,
            error: color_eyre::eyre::eyre!("failed to encode thread/inject_items payload: {err}"),
        }
    })?;
    let started = crate::app_server_session::fork_thread_with_request_handle(
        request_handle.clone(),
        config,
        parent_thread_id,
        thread_params_mode,
        remote_cwd_override,
    )
    .await
    .map_err(|err| SideThreadPrepareError {
        session: None,
        error: err,
    })?;
    let child_thread_id = started.session.thread_id;

    // Keep fork and boundary injection in one background operation so the App never observes a
    // side thread that can run before its inherited history is marked reference-only.
    let inject_result = request_handle
        .request_typed::<ThreadInjectItemsResponse>(ClientRequest::ThreadInjectItems {
            request_id: RequestId::String(format!("side-thread-inject-items-{}", Uuid::new_v4())),
            params: ThreadInjectItemsParams {
                thread_id: child_thread_id.to_string(),
                items: vec![boundary_item],
            },
        })
        .await;
    if let Err(err) = inject_result {
        return Err(SideThreadPrepareError {
            session: Some(started.session),
            error: color_eyre::eyre::eyre!(
                "thread/inject_items failed during TUI side conversation setup: {err}"
            ),
        });
    }
    Ok(started.session)
}
