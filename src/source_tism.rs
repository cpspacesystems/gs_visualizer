use libc::{O_RDWR, close, munmap, pthread_rwlock_rdlock, pthread_rwlock_unlock, shm_open};
use std::{
    io,
    mem::size_of,
    path::Path,
    ptr, slice,
    sync::atomic::{AtomicBool, AtomicU64, Ordering},
};

const TISM_MAJOR_VERSION: u8 = 2;

#[repr(C)]
struct Allocation {
    data_size: libc::size_t,
    major_version: u8,
    minor_version: u8,
    patch_version: u16,
    total_writes: AtomicU64,
    is_zombie: AtomicBool,
    rw_lock: libc::pthread_rwlock_t,
    timestamp: libc::timespec,
    data: u8,
}

const TISM_OVERHEAD: usize = size_of::<Allocation>() - size_of::<u8>();

pub fn read_once(address: &str) -> io::Result<Vec<u8>> {
    let mut source = DynamicTismSource::open(address)?;
    source.read()
}

struct DynamicTismSource {
    fd: libc::c_int,
    allocation: *mut Allocation,
}

impl DynamicTismSource {
    fn open(name: impl AsRef<Path>) -> io::Result<Self> {
        let name_bytes = name.as_ref().as_os_str().as_encoded_bytes();
        let mut name_bytes = name_bytes.to_vec();
        name_bytes.push(0);

        #[cfg(target_os = "macos")]
        name_bytes.insert(0, b'/');

        unsafe {
            let c_str = name_bytes.as_ptr() as *const libc::c_char;

            #[cfg(target_os = "macos")]
            let fd = shm_open(c_str, O_RDWR);
            #[cfg(target_os = "linux")]
            let fd = shm_open(c_str, O_RDWR, 0);

            if fd < 0 {
                return Err(io::Error::last_os_error());
            }

            let header = libc::mmap(
                ptr::null_mut(),
                size_of::<libc::size_t>(),
                libc::PROT_WRITE | libc::PROT_READ,
                libc::MAP_SHARED,
                fd,
                0,
            );

            if header == libc::MAP_FAILED {
                let err = io::Error::last_os_error();
                close(fd);
                return Err(err);
            }

            let data_size = *(header as *const libc::size_t);
            munmap(header, size_of::<libc::size_t>());

            let allocation = libc::mmap(
                ptr::null_mut(),
                TISM_OVERHEAD + data_size,
                libc::PROT_WRITE | libc::PROT_READ,
                libc::MAP_SHARED,
                fd,
                0,
            );

            if allocation == libc::MAP_FAILED {
                let err = io::Error::last_os_error();
                close(fd);
                return Err(err);
            }

            let allocation = allocation as *mut Allocation;

            if (*allocation).major_version != TISM_MAJOR_VERSION {
                let err = io::Error::new(io::ErrorKind::InvalidData, "TISM major version mismatch");
                munmap(allocation.cast(), TISM_OVERHEAD + data_size);
                close(fd);
                return Err(err);
            }

            if (*allocation).is_zombie.load(Ordering::Acquire) {
                let err = io::Error::new(io::ErrorKind::NotFound, "allocation is a zombie");
                munmap(allocation.cast(), TISM_OVERHEAD + data_size);
                close(fd);
                return Err(err);
            }

            Ok(Self { fd, allocation })
        }
    }

    fn read(&mut self) -> io::Result<Vec<u8>> {
        unsafe {
            match pthread_rwlock_rdlock(&raw mut (*self.allocation).rw_lock) {
                0 => {}
                e => return Err(io::Error::from_raw_os_error(e)),
            }

            let data_size = (*self.allocation).data_size;
            let bytes = slice::from_raw_parts(&raw const (*self.allocation).data, data_size);
            let payload = bytes.to_vec();

            match pthread_rwlock_unlock(&raw mut (*self.allocation).rw_lock) {
                0 => Ok(payload),
                e => Err(io::Error::from_raw_os_error(e)),
            }
        }
    }
}

impl Drop for DynamicTismSource {
    fn drop(&mut self) {
        unsafe {
            let data_size = (*self.allocation).data_size;
            let _ = close(self.fd);
            let _ = munmap(self.allocation.cast(), TISM_OVERHEAD + data_size);
        }
    }
}
