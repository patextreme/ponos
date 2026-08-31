//! Bookkeeping of live child process groups for one run.
//!
//! Pure pid data — register a group leader at spawn, deregister it at
//! natural death, snapshot the live set — with no I/O and no opinions
//! about who kills what. The composition root's signal monitor is the
//! type's one consumer: when a second SIGINT/SIGTERM hard-exits the
//! process on a path where no destructor can run, it snapshots the
//! registry and kills every group itself. Teardown never consults it;
//! the registry is the escape hatch's map, not the run's kill
//! mechanism (the normal paths kill and reap through their own
//! guards).

use std::sync::Mutex;

/// Registered live child process-group leaders.
#[derive(Debug, Default)]
pub struct ProcessGroups {
    pids: Mutex<Vec<u32>>,
}

impl ProcessGroups {
    /// An empty registry.
    pub fn new() -> Self {
        Self::default()
    }

    /// Record a live group-leader pid. Idempotent per pid: a duplicate
    /// registration must not grow the set the sweep walks.
    pub fn register(&self, pid: u32) {
        let mut pids = self.lock();
        if !pids.contains(&pid) {
            pids.push(pid);
        }
    }

    /// Forget a pid. Idempotent: deregistering an already-forgotten pid
    /// is a no-op, and a dead pid left behind (an unanticipated exit
    /// path) only degrades to a wasted sweep syscall — `ESRCH` — never
    /// a wrong kill unless the OS has already reused the pid.
    pub fn deregister(&self, pid: u32) {
        self.lock().retain(|p| *p != pid);
    }

    /// The currently-registered pids as a fresh copy, so callers act
    /// on a snapshot without holding the lock.
    pub fn snapshot(&self) -> Vec<u32> {
        self.lock().clone()
    }

    /// A poisoned lock means some thread panicked mid-update; the
    /// bookkeeping is still consistent enough to kill children by, and
    /// the one reader that can race this (the second-signal sweep) is
    /// on its way to `process::exit` anyway.
    fn lock(&self) -> std::sync::MutexGuard<'_, Vec<u32>> {
        self.pids.lock().unwrap_or_else(std::sync::PoisonError::into_inner)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn register_then_snapshot_lists_the_pid() {
        let groups = ProcessGroups::new();
        groups.register(111);
        groups.register(222);
        assert_eq!(groups.snapshot(), vec![111, 222]);
    }

    #[test]
    fn deregister_removes_the_pid() {
        let groups = ProcessGroups::new();
        groups.register(111);
        groups.register(222);
        groups.deregister(111);
        assert_eq!(groups.snapshot(), vec![222]);
    }

    #[test]
    fn double_deregister_is_a_noop() {
        let groups = ProcessGroups::new();
        groups.register(111);
        groups.deregister(111);
        groups.deregister(111);
        assert!(groups.snapshot().is_empty());
    }

    #[test]
    fn duplicate_register_collapses() {
        let groups = ProcessGroups::new();
        groups.register(111);
        groups.register(111);
        assert_eq!(groups.snapshot(), vec![111]);
    }

    #[test]
    fn snapshot_is_a_copy_not_a_view() {
        let groups = ProcessGroups::new();
        groups.register(111);
        let mut snap = groups.snapshot();
        snap.clear();
        assert_eq!(groups.snapshot(), vec![111]);
    }
}
