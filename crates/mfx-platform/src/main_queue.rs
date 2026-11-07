//! Esecuzione sul thread principale (macOS) con libdispatch, senza attendere il risultato.
//!
//! Le API Carbon delle hotkey vanno usate dal thread principale, dove gira il run loop di Cocoa.
//! Mai in modo sincrono: il thread principale può essere fermo in attesa dell'engine, e un
//! `dispatch_sync` dall'engine finirebbe in un deadlock.

use std::ffi::c_void;

#[repr(C)]
struct DispatchQueue {
    _private: [u8; 0],
}

unsafe extern "C" {
    /// La coda principale: `dispatch_get_main_queue()` è una macro che ne prende l'indirizzo.
    static _dispatch_main_q: DispatchQueue;
    fn dispatch_async_f(queue: *const DispatchQueue, context: *mut c_void, work: extern "C" fn(*mut c_void));
}

type Job = Box<dyn FnOnce() + Send>;

extern "C" fn trampoline(ctx: *mut c_void) {
    // SAFETY: `ctx` è il Box creato in run_on_main, consumato una sola volta.
    let job = unsafe { Box::from_raw(ctx as *mut Job) };
    job();
}

/// Accoda `f` sul thread principale. I lavori partono nell'ordine in cui sono accodati.
pub fn run_on_main(f: impl FnOnce() + Send + 'static) {
    let job: Box<Job> = Box::new(Box::new(f));
    // SAFETY: coda principale statica; il contesto viene liberato dal trampolino.
    unsafe { dispatch_async_f(&raw const _dispatch_main_q, Box::into_raw(job).cast(), trampoline) };
}
