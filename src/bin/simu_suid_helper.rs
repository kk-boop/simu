use std::ffi::{CStr, CString, OsStr};
use std::fs::File;
use std::io::{stdin, stdout, ErrorKind, Read, Write};
use std::os::unix::ffi::OsStrExt;

use libc::{getpwnam, initgroups, setgid, setgroups, setuid};
use pam::{Authenticator, PamResult};
use simu::{Directory, DirectoryEntry, ReturnCode};

const PAM_SERVICE: &str = "login";
const BUF_SIZE: usize = 4096;

fn main() {
    let inp = read_input();
    if inp.is_err() {
        // no stdin reading happened
        panic!("Can't read input");
    }
    let (username, password, path, type_) = inp.unwrap();

    #[cfg(feature = "root-safeguard")]
    {
        // Refuse to attempt root authentication making the safeguard exceptional
        if username.as_bytes() == b"root" {
            panic!("Refusing to authenticate as root");
        }
    }

    unsafe {
        let res = setuid(0); // Become root before calling PAM, in case Linux capabilities are used instead of SUID bit.
        if res < 0 {
            panic!("SUID helper binary not SUID or setcap cap_setuid,cap_setgid+ep!");
        }
    }

    //eprintln!("We wish to become '{}', so i can read file '{}'", username.to_string_lossy(), path.to_string_lossy());
    let res = test_auth(&username, &password);
    if let Err(e) = res {
        panic!("PAM failed: {}", e);
    }
    if res.unwrap() == 0 {
        login_failed();
    }

    let ret = become_user(&username);
    if ret < 0 {
        panic!("Could not switch user");
    }

    if type_.as_bytes() == "DIR".as_bytes() {
        read_dir_to_stdout(&path);
    } else {
        read_file_to_stdout(&path);
    }
}

fn read_input() -> Result<(CString, CString, CString, CString), ()> {
    let mut full_buf = Vec::with_capacity(BUF_SIZE);
    loop {
        let mut buf = [0; BUF_SIZE];
        let _count = stdin().read(&mut buf);
        full_buf.extend(buf.into_iter());
        if full_buf
            .iter()
            .filter(|b| **b == 0)
            .map(|_b| 1usize)
            .sum::<usize>()
            > 2
        {
            // we found all three nullbytes
            let mut strings = take_cstrings(&full_buf);

            if strings.len() < 3 {
                // Not enough input cstrings found? dying.
                return Err(());
            } else if strings.len() > 4 {
                // weird number of inputs!!!
                eprint!("Weird count of inputs in stdin!");
            }
            return Ok((
                strings.remove(0),
                strings.remove(0),
                strings.remove(0),
                strings.remove(0),
            ));
        }
    }
}

fn take_cstrings(c_str: &[u8]) -> Vec<CString> {
    c_str
        .split(|b| *b == 0)
        .filter(|s| !s.is_empty())
        .map(|s| s.to_owned())
        .map(CString::new)
        .map(|s| s.unwrap())
        .collect()
}

/**
 * Runs user detail through PAM, returns either PAM interaction errors, or Ok(0) on bad auth, or Ok(1) on good auth.
 */
fn test_auth(username: &CStr, password: &CStr) -> PamResult<i32> {
    let mut authenticator =
        Authenticator::with_handler(PAM_SERVICE, CStringConverse::new(username, password))
            .expect("Failed to init PAM");
    if authenticator.authenticate().is_err() {
        // Not an 'error' but failed authentication
        return Ok(0);
    }
    Ok(1)
}

// TODO: Pin + zero on drop somewhat needed to limit spraying heap with copies of passwords and usernames.
// Impact is limited in this application, but would be good taste.
struct CStringConverse {
    login: CString,
    login_as_str: String,
    passwd: CString,
}

impl CStringConverse {
    fn new(login: &CStr, passwd: &CStr) -> CStringConverse {
        CStringConverse {
            login: login.to_owned(),
            login_as_str: login.to_string_lossy().to_string(),
            passwd: passwd.to_owned(),
        }
    }
}

impl pam::Converse for CStringConverse {
    fn prompt_echo(&mut self, _msg: &CStr) -> std::result::Result<CString, ()> {
        Ok(self.login.clone())
    }

    fn prompt_blind(&mut self, _msg: &CStr) -> std::result::Result<CString, ()> {
        Ok(self.passwd.clone())
    }

    fn info(&mut self, msg: &CStr) {
        eprintln!("[PAM INFO] {}", msg.to_string_lossy());
    }

    fn error(&mut self, msg: &CStr) {
        eprintln!("[PAM ERROR] {}", msg.to_string_lossy());
    }

    fn username(&self) -> &str {
        &self.login_as_str // This is internally only used for open_session, which we do not use.
    }
}

/**
 * This has been developed and tested for Linux, but in
 * theory should also work on BSDs and SVr4-compat
 * systems (only `initgroups` is not POSIX).
 *
 * username: system username to become
 */
fn become_user(username: &CStr) -> i32 {
    let pwent = unsafe { getpwnam(username.as_ptr()) };
    if pwent.is_null() {
        return -1; // pwent is generally static memory, if this fails, you have some really crazy issues and this is the least of your concern
    }
    let gid = unsafe { (*pwent).pw_gid };
    let uid = unsafe { (*pwent).pw_uid };

    unsafe {
        // This setgid is needed even though we are using setgroups,
        // as otherwise this group is later reinserted to supplemental groups.
        let ret = setgid(gid);
        if ret < 0 {
            //println!("setgid returned {}", errno());
            return ret;
        }
    }
    //println!("After setgid, im \t'{}'", &get_id()); // uid=1000(keerup) gid=1000(keerup) euid=0(root) groups=1000(keerup),24(cdrom),25(floppy),27(sudo),29(audio),30(dip),44(video),46(plugdev),109(netdev),112(bluetooth),116(scanner),130(libvirt)

    unsafe {
        // Remove all current supplemental groups
        let ret = setgroups(0, std::ptr::null());
        if ret < 0 {
            //println!("setgroups returned {}", errno());
            return ret;
        }
    }
    //println!("After setgroups, im \t'{}'", &get_id()); // uid=1000(keerup) gid=30033(keerup_test) euid=0(root) groups=30033(keerup_test)

    unsafe {
        // Add all user's supplemental groups
        let ret = initgroups(username.as_ptr(), gid);
        if ret < 0 {
            //println!("initgroups returned {}", errno());
            return ret;
        }
    }
    //println!("After initgroups, im \t'{}'", &get_id()); // uid=1000(keerup) gid=30033(keerup_test) euid=0(root) groups=30033(keerup_test),25(floppy),116(scanner),30034(keerup_test2)

    unsafe {
        // Set to requested user ID
        let ret = setuid(uid);
        if ret < 0 {
            //println!("setuid returned {}", errno());
            return ret;
        }
    }
    //println!("After setuid, im \t'{}'", &get_id()); // uid=30033(keerup_test) gid=30033(keerup_test) groups=30033(keerup_test),25(floppy),116(scanner),30034(keerup_test2)

    #[cfg(feature = "root-safeguard")]
    unsafe {
        // As a safeguard, lets attempt to become root again
        // This should fail in the common case
        let ret = setuid(0);
        if ret == 0 {
            panic!("We should not be able to become root again!");
        }
    }

    // Switch of user complete

    0
}

fn read_file_to_stdout(path: &CString) {
    let path_os = OsStr::from_bytes(path.as_bytes()); // possibly removes need for UTF-8 paths? need to test
    let mut file = match File::open(&*path_os) {
        Err(e) => match e.kind() {
            ErrorKind::NotFound => file_not_found(),
            ErrorKind::PermissionDenied => permission_denied(),
            _ => unknown_error(),
        },
        Ok(f) => f,
    };
    if let Ok(meta) = file.metadata() {
        if meta.is_dir() {
            unexpected_type();
        }
    }
    loop {
        let mut buf = [0; BUF_SIZE];
        let count = file.read(&mut buf);
        match count {
            Ok(sz) => {
                if sz == 0 {
                    return; // EOF
                }
                loop {
                    let res = stdout().write_all(&buf[0..sz]);
                    if let Err(e) = res {
                        match e.kind() {
                            ErrorKind::Interrupted => {
                                continue;
                            } // this should be retried, but cannot let repeat outer loop, as that mangles the file
                            ErrorKind::BrokenPipe => {
                                panic!("File closed, client likely lost");
                            }
                            _ => {
                                panic!("error while writing to output: {:?}", e.kind());
                            }
                        }
                    } else {
                        break;
                    }
                }
            }
            Err(err) => {
                // read failed, lets check if it's potentially recoverable
                match err.kind() {
                    ErrorKind::Interrupted => {
                        continue;
                    }
                    _ => {
                        panic!("file reading failed! '{:?}'", err.kind())
                    }
                }
            }
        }
    }
}

fn read_dir_to_stdout(path: &CString) {
    let path_os = OsStr::from_bytes(path.as_bytes()); // possibly removes need for UTF-8 paths? need to test
    let dir = Directory(match std::fs::read_dir(path_os) {
        Ok(it) => it
            .filter(|ent| ent.is_ok())
            .map(|ent| DirectoryEntry::from(ent.unwrap()))
            .collect(),
        Err(_) => {
            // doesnt exist, no perms, or isnt dir
            file_not_found();
        }
    });
    if stdout()
        .write_all(&bincode::serialize(&dir).unwrap())
        .is_err()
    {
        file_not_found();
    }
}

fn file_not_found() -> ! {
    eprint!("File not found!");
    std::process::exit(ReturnCode::FileNotFound as i32)
}

fn permission_denied() -> ! {
    eprint!("Permission denied!");
    std::process::exit(ReturnCode::PermissionDenied as i32)
}

fn unknown_error() -> ! {
    eprint!("Unknown IO error!");
    std::process::exit(ReturnCode::Unknown as i32)
}

fn login_failed() -> ! {
    eprint!("Login failed!");
    std::process::exit(ReturnCode::LoginFailed as i32)
}

fn unexpected_type() -> ! {
    eprint!("Unexpected type!");
    std::process::exit(ReturnCode::UnexpectedType as i32)
}
