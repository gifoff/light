#[allow(clippy::module_inception)]
#[cfg(test)]
mod tests {
    use crate::async_runtime::*;
    use std::any::Any;
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::time::Duration;

    #[test]
    fn post_to_ui_without_install_errors() {
        let result = post_to_ui(|| {});
        match result {
            Err(TaskError::NotRunning) => {}
            other => panic!("expected NotRunning, got {other:?}"),
        }
    }

    #[test]
    fn spawn_runs_closure() {
        static RAN: AtomicBool = AtomicBool::new(false);
        let handle = spawn(|_ctx| RAN.store(true, Ordering::SeqCst));
        handle.join().expect("spawned thread must not panic");
        assert!(RAN.load(Ordering::SeqCst), "closure should have run");
    }

    #[test]
    fn spawn_returns_joinable_handle() {
        let handle = spawn(|_ctx| {});
        handle.join().expect("empty closure must not panic");
    }

    #[test]
    fn abort_stops_task() {
        static CANCELLED: AtomicBool = AtomicBool::new(false);
        let handle = spawn(|ctx| {
            while !ctx.is_cancelled() {
                std::thread::sleep(Duration::from_millis(1));
            }
            CANCELLED.store(true, Ordering::SeqCst);
        });
        handle.abort();
        handle.join().expect("aborted thread must finish");
        assert!(
            CANCELLED.load(Ordering::SeqCst),
            "task should observe cancellation"
        );
    }

    #[test]
    fn post_update_without_install_errors() {
        struct S;
        impl light_vdom::component::Component for S {
            fn as_any_mut(&mut self) -> &mut dyn std::any::Any {
                self
            }
            fn render(&self) -> light_vdom::vdom::VElement {
                light_vdom::vdom::VElement::leaf(
                    "",
                    light_vdom::vdom::NodeKind::Text {
                        content: "noop".into(),
                    },
                )
            }
        }
        let result = post_update::<S>(|_| {});
        match result {
            Err(TaskError::NotRunning) => {}
            other => panic!("expected NotRunning, got {other:?}"),
        }
    }

    /// Тест 4.2: Debug-представление `Action` содержит `"Action"`
    /// и `"target"` (TypeId не паникует при форматировании).
    #[test]
    fn action_debug_contains_action_and_target() {
        let action = UserEvent::Action {
            f: Box::new(|_any: &mut dyn Any| {}),
            target: std::any::TypeId::of::<u8>(),
            token: None,
        };
        let s = format!("{action:?}");
        assert!(s.contains("Action"), "Debug must name the variant: {s}");
        assert!(
            s.contains("target"),
            "Debug must include the `target` field: {s}"
        );
        assert!(
            s.contains("<closure>"),
            "Debug must render the closure placeholder: {s}"
        );
    }

    #[test]
    fn task_error_enum_unchanged() {
        // TaskError должен сохранять исходный набор вариантов:
        // NotRunning, Closed, AlreadyInstalled.
        let not_running = TaskError::NotRunning;
        let closed = TaskError::Closed;
        let already = TaskError::AlreadyInstalled;

        assert_eq!(not_running.to_string(), "the event loop is not running");
        assert_eq!(closed.to_string(), "the event loop is closed");
        assert_eq!(
            already.to_string(),
            "the event loop proxy is already installed"
        );
    }
}
