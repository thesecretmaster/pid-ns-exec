/* This program take another program as an argument, wraps it in a linux `pid_namespace`
 * and runs it.
 *
 * PID namespaces require CAP_SYS_ADMIN so this executable must have that capability, however
 * that will be dropped prior to executing its argument.
 */

use std::env;

// Passed to `clone`
pub extern "C" fn exec(_: *mut libc::c_void) -> libc::c_int {
    use std::process::Command;
    use std::os::unix::process::CommandExt;
    use caps::CapSet;

    // First, drop capabilities
    caps::clear(None, CapSet::Permitted).expect("Could not drop caps");

    // Grab args again (pre-validated in `main`)
    let args: Vec<String> = env::args().collect();

    // Run command
    Command::new(&args[1]).args(&args[2..args.len()]).exec();
    0 // Never reached, the whole proccesses memory image has been replaced at this point
}


fn main() {
    use errno::errno;

    // Validate argument length
    let args: Vec<String> = env::args().collect();
    if args.len() <= 1 {
        panic!("You need to pass some args")
    }

    // Create a thread to exec in
    // I think 4096 should be more than enough for the stack
    const STACK_SIZE: usize = 4096;
    let layout = std::alloc::Layout::from_size_align(STACK_SIZE, std::mem::align_of::<u64>()).expect("Could not generate layout");
    let stack: *mut u8 = unsafe { std::alloc::alloc(layout).offset(STACK_SIZE.try_into().unwrap()) };
    let pid = unsafe {
        libc::clone(exec,
                    stack as *mut libc::c_void,
                    // Flags are stolen from `fork` with NEWPID added
                    libc::CLONE_NEWPID | libc::CLONE_CHILD_CLEARTID | libc::CLONE_CHILD_SETTID | libc::SIGCHLD,
                    std::ptr::null_mut())
    };

    // Check for success, otherwise print PID
    if pid <= 0 {
        let errno = errno();
        println!("Error {}: {}", errno.0, errno);
    } else {
        println!("Forked to PID {}", pid);
    }
}
