//! Deadline-bound discovery. Only a coverage-validated no-match permits launch.
use std::{
    process::Command,
    thread,
    time::{Duration, Instant},
};
use uitree::{SaveUIElement, StaleTree, TreeService};

#[derive(Debug, thiserror::Error)]
pub enum AppControlError {
    #[error(transparent)]
    Stale(#[from] StaleTree),
    #[error("Invalid application XPath: {0}")]
    InvalidQuery(String),
    #[error("Application operation deadline expired: {0}")]
    Deadline(String),
    #[error("Failed to activate application: {0}")]
    Activation(String),
    #[error("Failed to launch '{path}': {reason}")]
    Spawn { path: String, reason: String },
}

trait Backend {
    type Element;
    fn now(&self) -> Instant {
        Instant::now()
    }
    fn wait(&mut self, duration: Duration) {
        thread::sleep(duration);
    }
    fn discover(
        &mut self,
        xpath: &str,
        deadline: Instant,
    ) -> Result<Option<Self::Element>, AppControlError>;
    fn launch(&mut self, path: &str, deadline: Instant) -> Result<(), AppControlError>;
    fn activate(
        &mut self,
        element: &Self::Element,
        deadline: Instant,
    ) -> Result<(), AppControlError>;
}

fn launch_or_activate<B: Backend>(
    backend: &mut B,
    path: &str,
    xpath: &str,
    deadline: Instant,
) -> Result<B::Element, AppControlError> {
    let mut launched = false;
    loop {
        let found = backend.discover(xpath, deadline)?;
        if backend.now() >= deadline {
            return Err(AppControlError::Deadline(
                "discovery or launch polling".into(),
            ));
        }
        if let Some(element) = found {
            backend.activate(&element, deadline)?;
            return Ok(element);
        }
        if !launched {
            backend.launch(path, deadline)?;
            launched = true;
        }
        let remaining = deadline.saturating_duration_since(backend.now());
        if remaining.is_zero() {
            return Err(AppControlError::Deadline(
                "waiting for the launched application".into(),
            ));
        }
        backend.wait(remaining.min(Duration::from_millis(100)));
    }
}

struct Desktop<'a> {
    service: &'a TreeService,
    title: Option<&'a str>,
}
impl Backend for Desktop<'_> {
    type Element = SaveUIElement;
    fn discover(
        &mut self,
        xpath: &str,
        deadline: Instant,
    ) -> Result<Option<SaveUIElement>, AppControlError> {
        self.service
            .snapshot()
            .query(xpath)
            .map_err(AppControlError::InvalidQuery)?;
        self.service.membership(deadline).map_err(|mut e| {
            e.scope = self.title.map(str::to_owned);
            AppControlError::Stale(e)
        })?;
        let tree = self.service.ensure_query(xpath, self.title, deadline)?;
        Ok(tree
            .query(xpath)
            .map_err(AppControlError::InvalidQuery)?
            .first()
            .map(|p| (*p).clone()))
    }
    fn launch(&mut self, path: &str, deadline: Instant) -> Result<(), AppControlError> {
        let executable = path.to_owned();
        crate::deadline_worker::run(deadline, move || {
            Command::new(executable)
                .spawn()
                .map(|_| ())
                .map_err(|e| e.to_string())
        })
        .map_err(|e| match e {
            crate::deadline_worker::WorkerError::Deadline => AppControlError::Deadline(
                "process creation; an OS call already started may finish later".into(),
            ),
            crate::deadline_worker::WorkerError::Failed(reason) => AppControlError::Spawn {
                path: path.into(),
                reason,
            },
        })
    }
    fn activate(
        &mut self,
        element: &SaveUIElement,
        deadline: Instant,
    ) -> Result<(), AppControlError> {
        let service = self.service.clone();
        let id = element.get_runtime_id().to_vec();
        let index = service
            .snapshot()
            .index_for_id(&id)
            .ok_or_else(|| AppControlError::Activation("target removed".into()))?;
        crate::deadline_worker::run(deadline, move || {
            let result = (|| {
                if service.snapshot().index_for_id(&id) != Some(index) {
                    return Err("target removed or replaced".into());
                }
                let live = service.resolve_live_expected(&id, Some(index))?;
                if Instant::now() >= deadline {
                    return Err("activation expired before focus".into());
                }
                live.set_focus().map_err(|e| e.to_string())
            })();
            service.invalidate_action(&id);
            result
        })
        .map_err(|e| match e {
            crate::deadline_worker::WorkerError::Deadline => AppControlError::Deadline(
                "activation; a provider call already started may finish later".into(),
            ),
            crate::deadline_worker::WorkerError::Failed(reason) => {
                AppControlError::Activation(reason)
            }
        })
    }
}

pub fn launch_or_activate_application(
    service: &TreeService,
    title: Option<&str>,
    path: &str,
    xpath: &str,
    deadline: Instant,
) -> Result<SaveUIElement, AppControlError> {
    launch_or_activate(&mut Desktop { service, title }, path, xpath, deadline)
}

#[cfg(test)]
mod tests {
    use super::*;
    struct Fake {
        now: Instant,
        results: std::collections::VecDeque<Result<Option<u8>, AppControlError>>,
        launches: usize,
        activations: usize,
        deadlines: Vec<Instant>,
    }
    impl Backend for Fake {
        type Element = u8;
        fn now(&self) -> Instant {
            self.now
        }
        fn wait(&mut self, d: Duration) {
            self.now += d;
        }
        fn discover(&mut self, _: &str, d: Instant) -> Result<Option<u8>, AppControlError> {
            self.deadlines.push(d);
            self.results.pop_front().unwrap_or(Ok(None))
        }
        fn launch(&mut self, _: &str, d: Instant) -> Result<(), AppControlError> {
            self.deadlines.push(d);
            self.launches += 1;
            Ok(())
        }
        fn activate(&mut self, _: &u8, d: Instant) -> Result<(), AppControlError> {
            self.deadlines.push(d);
            self.activations += 1;
            Ok(())
        }
    }
    fn fake(results: Vec<Result<Option<u8>, AppControlError>>) -> Fake {
        Fake {
            now: Instant::now(),
            results: results.into(),
            launches: 0,
            activations: 0,
            deadlines: vec![],
        }
    }
    #[test]
    fn existing_descendant_never_launches() {
        let mut b = fake(vec![Ok(Some(7))]);
        let d = b.now + Duration::from_secs(1);
        assert_eq!(launch_or_activate(&mut b, "app", "//Button", d).unwrap(), 7);
        assert_eq!((b.launches, b.activations), (0, 1));
    }
    #[test]
    fn only_clean_absence_launches_once_with_one_deadline() {
        let mut b = fake(vec![Ok(None), Ok(None), Ok(Some(7))]);
        let d = b.now + Duration::from_secs(1);
        launch_or_activate(&mut b, "app", "//Button", d).unwrap();
        assert_eq!((b.launches, b.activations), (1, 1));
        assert!(b.deadlines.iter().all(|&v| v == d));
    }
    #[test]
    fn stale_and_invalid_discovery_never_launch() {
        for error in [
            AppControlError::InvalidQuery("bad".into()),
            AppControlError::Stale(StaleTree {
                reason: "provider failed".into(),
                scope: None,
                revision: 4,
                coverage: "dirty".into(),
            }),
        ] {
            let mut b = fake(vec![Err(error)]);
            let d = b.now + Duration::from_secs(1);
            assert!(launch_or_activate(&mut b, "app", "query", d).is_err());
            assert_eq!(b.launches, 0);
        }
    }
    #[test]
    fn polling_does_not_restart_budget() {
        let mut b = fake(vec![]);
        let d = b.now + Duration::from_millis(250);
        assert!(matches!(
            launch_or_activate(&mut b, "app", "query", d),
            Err(AppControlError::Deadline(_))
        ));
        assert_eq!(b.now, d);
        assert_eq!(b.launches, 1);
        assert!(b.deadlines.iter().all(|&v| v == d));
    }
    #[test]
    fn expired_discovery_does_not_launch() {
        let mut b = fake(vec![]);
        let d = b.now;
        assert!(matches!(
            launch_or_activate(&mut b, "app", "query", d),
            Err(AppControlError::Deadline(_))
        ));
        assert_eq!(b.launches, 0);
    }
}
