//! Evento auto-reset su semaforo Mach: il meccanismo di risveglio di tutti i thread di MicFX su macOS
//! (SPEC §4). `set` non prende lock e non alloca: si può chiamare dal thread audio di CoreAudio.
//!
//! `armed` fa da evento auto-reset sopra il semaforo contatore: più `set` prima di un `wait`
//! producono un solo risveglio, come `SetEvent` su Windows.

use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

type KernReturn = i32;
type MachPort = u32;

#[repr(C)]
#[derive(Clone, Copy)]
struct MachTimespec {
    tv_sec: u32,
    tv_nsec: i32,
}

const KERN_SUCCESS: KernReturn = 0;
const KERN_ABORTED: KernReturn = 14;
const KERN_OPERATION_TIMED_OUT: KernReturn = 49;
const SYNC_POLICY_FIFO: i32 = 0;

unsafe extern "C" {
    static mach_task_self_: MachPort;
    fn semaphore_create(task: MachPort, semaphore: *mut MachPort, policy: i32, value: i32) -> KernReturn;
    fn semaphore_destroy(task: MachPort, semaphore: MachPort) -> KernReturn;
    fn semaphore_signal(semaphore: MachPort) -> KernReturn;
    fn semaphore_wait(semaphore: MachPort) -> KernReturn;
    fn semaphore_timedwait(semaphore: MachPort, wait_time: MachTimespec) -> KernReturn;
}

pub struct Event {
    sem: MachPort,
    armed: AtomicBool,
}

pub enum Wait {
    Signaled(usize),
    Timeout,
    Failed,
}

impl Event {
    pub fn new() -> std::io::Result<Self> {
        let mut sem: MachPort = 0;
        // SAFETY: crea un semaforo del task corrente; distrutto in Drop.
        let r = unsafe { semaphore_create(mach_task_self_, &mut sem, SYNC_POLICY_FIFO, 0) };
        if r != KERN_SUCCESS {
            return Err(std::io::Error::other(format!("semaphore_create: {r}")));
        }
        Ok(Self { sem, armed: AtomicBool::new(false) })
    }

    #[inline]
    pub fn set(&self) {
        if !self.armed.swap(true, Ordering::AcqRel) {
            // SAFETY: semaforo valido per tutta la vita di self.
            unsafe { semaphore_signal(self.sem) };
        }
    }

    /// Attende questo evento. `None` = senza limite.
    pub fn wait(&self, timeout_ms: Option<u32>) -> Wait {
        let deadline = timeout_ms.map(|ms| Instant::now() + Duration::from_millis(ms as u64));
        loop {
            let r = match deadline {
                // SAFETY: semaforo valido.
                None => unsafe { semaphore_wait(self.sem) },
                Some(d) => {
                    let left = d.saturating_duration_since(Instant::now());
                    let ts = MachTimespec { tv_sec: left.as_secs() as u32, tv_nsec: left.subsec_nanos() as i32 };
                    // SAFETY: semaforo valido.
                    unsafe { semaphore_timedwait(self.sem, ts) }
                }
            };
            match r {
                KERN_SUCCESS => {
                    // Reset dopo il risveglio: un `set` arrivato nel frattempo lascia il semaforo a 1
                    // e il prossimo `wait` torna subito, nessun risveglio perso.
                    self.armed.store(false, Ordering::Release);
                    return Wait::Signaled(0);
                }
                KERN_OPERATION_TIMED_OUT => return Wait::Timeout,
                KERN_ABORTED => continue,
                _ => return Wait::Failed,
            }
        }
    }
}

impl Drop for Event {
    fn drop(&mut self) {
        // SAFETY: semaforo creato in new() e mai distrutto altrove.
        unsafe { semaphore_destroy(mach_task_self_, self.sem) };
    }
}

impl crate::sink::Wake for Event {
    fn wake(&self) {
        self.set();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn auto_reset_and_timeout() {
        let e = Event::new().unwrap();
        e.set();
        e.set();
        assert!(matches!(e.wait(Some(10)), Wait::Signaled(0)));
        assert!(matches!(e.wait(Some(10)), Wait::Timeout));
    }

    #[test]
    fn wakes_another_thread() {
        let e = std::sync::Arc::new(Event::new().unwrap());
        let e2 = e.clone();
        let t = std::thread::spawn(move || matches!(e2.wait(Some(2000)), Wait::Signaled(0)));
        std::thread::sleep(Duration::from_millis(20));
        e.set();
        assert!(t.join().unwrap());
    }
}
