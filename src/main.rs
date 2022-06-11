/* This program take another program as an argument, wraps it in a linux `pid_namespace`
 * and runs it.
 *
 * PID namespaces require CAP_SYS_ADMIN so this executable must have that capability, however
 * that will be dropped prior to executing its argument.
 */

use std::env;



fn main() {
    // Validate argument length
    let args: Vec<String> = env::args().collect();
    if args.len() <= 1 {
        panic!("You need to pass some args")
    }

    println!("Launching PID NS root");
    let pid = unsafe { launch_thread(ns_init, MANAGER_STACK_SIZE, libc::CLONE_NEWPID) };

    // Check for success, otherwise print PID
    if pid <= 0 {
        println!("Error launching PID NS root {}", std::io::Error::last_os_error());
    } else {
        println!("PID NS root is at {}", pid);
        wait_for_children().unwrap();
    }
}

pub extern "C" fn ns_init(_: *mut libc::c_void) -> libc::c_int {
    use caps::CapSet;

    // First, drop capabilities
    caps::clear(None, CapSet::Permitted).expect("Could not drop caps");
    println!("Dropped all capabilities");

    // Create a thread to exec in
    let pid = unsafe { launch_thread(exec, RUNNER_STACK_SIZE, 0) };

    // Check for success, otherwise print PID
    if pid <= 0 {
        println!("Error launching application thread {}", std::io::Error::last_os_error());
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
        println!("Error executing application {}", std::io::Error::last_os_error());
    }
    panic!("Running after sucessful execv")
}

// Wait for all children of the current PID to exit
fn wait_for_children() -> Result<(), std::io::Error> {
    loop {
        let wait_retval = unsafe { libc::waitid(libc::P_ALL, 0, std::ptr::null_mut(), libc::WEXITED) };
        if wait_retval == -1 {
            let errno = std::io::Error::last_os_error();
            match errno.raw_os_error() {
                Some(libc::ECHILD) => return Ok(()),
                Some(libc::EAGAIN) => continue,
                Some(libc::EINTR) => continue,
                _ => return Err(errno)
            }
        }
    }
}


// Thread managment
// We launch with raw `clone` because we need to use its flags
// otherwise I'd avoid it. I'm hiding all that mess down here


// Tunable parameters:
// Generally don't touch these. Except maybe alignment but even
// then like.... why

// Page size (4KB should be a good guess)
// Used to align the fresh stacks
const PAGE_ALIGNMENT: usize = 4096;
// Small stack for the runner because it's sole purpose is
// to be re-imaged with `exec`
const RUNNER_STACK_SIZE: usize = 8192; // 8 KB
// Manager stack larger because I may eventually want more
// functionality there
const MANAGER_STACK_SIZE: usize = 4194304; // 4 MB

unsafe fn launch_thread(target: extern "C" fn(*mut libc::c_void) -> libc::c_int, stack_size: usize, additional_flags: libc::c_int) -> libc::c_int {
    libc::clone(target,
                gen_stack(stack_size) as *mut libc::c_void,
                // Base flags are stolen from `fork`
                // Plus, if I don't use them my wait calls fail
                libc::CLONE_CHILD_CLEARTID | libc::CLONE_CHILD_SETTID | libc::SIGCHLD | additional_flags,
                std::ptr::null_mut()
                )
}

unsafe fn gen_stack(stack_size: usize) -> *mut u8 {
    // Not totally sure what alignment to select so going with page alignment
    let layout = std::alloc::Layout::from_size_align(stack_size, PAGE_ALIGNMENT).expect("Could not generate layout");
    std::alloc::alloc_zeroed(layout).offset(stack_size.try_into().expect("Could not allocate stack for new process"))
}

