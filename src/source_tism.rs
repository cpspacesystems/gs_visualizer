use std::io;
use tism::dynamic::{self, DynamicBorrowedSharedMemory};

pub struct TismSource {
    inner: DynamicBorrowedSharedMemory,
}

impl TismSource {
    pub fn open(address: &str) -> io::Result<Self> {
        Ok(Self {
            inner: dynamic::open(address)?,
        })
    }

    pub fn read(&mut self) -> io::Result<Vec<u8>> {
        self.inner.read()
    }
}
