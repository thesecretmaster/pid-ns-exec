/* This program take another program as an argument, wraps it in a linux `pid_namespace`
 * and runs it.
 *
 * PID namespaces require CAP_SYS_ADMIN so this executable must have that capability, however
 * that will be dropped prior to executing its argument.
 */

use std::env;
use errno::errno;


fn wait_for_children() -> Result<(), errno::Errno> {
    loop {
        let wait_retval = unsafe { libc::waitid(libc::P_ALL, 0, std::ptr::null_mut(), libc::WEXITED) };
        if wait_retval == -1 {
            let errno = errno();
            match errno.0 {
                libc::ECHILD => return Ok(()),
                libc::EAGAIN => continue,
                libc::EINTR => continue,
                _ => return Err(errno)
            }
        }
    }
}

unsafe fn gen_stack() -> *mut u8 {
    // I think 4096 should be more than enough for the stack
    const STACK_SIZE: usize = 4096;

    let layout = std::alloc::Layout::from_size_align(STACK_SIZE, std::mem::align_of::<u64>()).expect("Could not generate layout");
    let stack: *mut u8 = std::alloc::alloc_zeroed(layout).offset(STACK_SIZE.try_into().unwrap());
    stack
}

pub extern "C" fn ns_init(_: *mut libc::c_void) -> libc::c_int {
    use caps::CapSet;

    // First, drop capabilities
    caps::clear(None, CapSet::Permitted).expect("Could not drop caps");
    println!("Dropped all capabilities");

    // Create a thread to exec in
    let pid = unsafe {
        libc::clone(exec,
                    gen_stack() as *mut libc::c_void,
                    libc::CLONE_CHILD_CLEARTID | libc::CLONE_CHILD_SETTID | libc::SIGCHLD,
                    std::ptr::null_mut())
    };

    // Check for success, otherwise print PID
    if pid <= 0 {
        let errno = errno();
        println!("Error launching application thread {}: {}", errno.0, errno);
    } else {
        println!("Launched application thread at {}", pid);
        wait_for_children().unwrap();
    }
    0
}

pub extern "C" fn exec(_: *mut libc::c_void) -> libc::c_int {
    // Grab args again (pre-validated in `main`)
    let args: Vec<String> = env::args().collect();
    println!("Running {:?}", args);

    // Convert command to CStr
    let prog_ptr: std::ffi::CString = std::ffi::CString::new(args[1].clone()).unwrap();
    let args_cstr: Vec<std::ffi::CString> = (&args[1..args.len()]).into_iter().map(|arg| std::ffi::CString::new(arg.clone()).unwrap()).collect::<Vec<std::ffi::CString>>();
    let mut args_ptr: Vec<*const libc::c_char> = args_cstr.iter().map(|arg| arg.as_ptr()).collect::<Vec<*const libc::c_char>>();
    // Needs to be null terminated
    args_ptr.push(std::ptr::null());

    // Replace running process with command
    let rv = unsafe { libc::execvp(prog_ptr.as_ptr(), args_ptr.as_ptr()) };

    if rv != 0 {
        let errno = errno();
        println!("Error executing application {}: {}", errno.0, errno);
    }
    panic!("Running after sucessful execv")
}


fn main() {
    // Validate argument length
    let args: Vec<String> = env::args().collect();
    if args.len() <= 1 {
        panic!("You need to pass some args")
    }

    println!("Launching PID NS root");
    // Create a thread to exec in
    let pid = unsafe {
        libc::clone(ns_init,
                    gen_stack() as *mut libc::c_void,
                    // Flags are stolen from `fork` with NEWPID added
                    libc::CLONE_CHILD_CLEARTID | libc::CLONE_CHILD_SETTID | libc::SIGCHLD | libc::CLONE_NEWPID,
                    std::ptr::null_mut())
    };

    // Check for success, otherwise print PID
    if pid <= 0 {
        let errno = errno();
        println!("Error launching PID NS root {}: {}", errno.0, errno);
    } else {
        println!("PID NS root is at {}", pid);
        wait_for_children().unwrap();
    }
}
