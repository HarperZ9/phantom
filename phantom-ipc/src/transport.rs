#[cfg(windows)]
extern "system" {
    fn ReadFile(
        hFile: isize,
        lpBuffer: *mut u8,
        nNumberOfBytesToRead: u32,
        lpNumberOfBytesRead: *mut u32,
        lpOverlapped: *mut u8,
    ) -> i32;
    fn WriteFile(
        hFile: isize,
        lpBuffer: *const u8,
        nNumberOfBytesToWrite: u32,
        lpNumberOfBytesWritten: *mut u32,
        lpOverlapped: *mut u8,
    ) -> i32;
    fn FlushFileBuffers(hFile: isize) -> i32;
    fn CloseHandle(hObject: isize) -> i32;
}

#[cfg(windows)]
pub struct PipeStream {
    handle: isize,
    owns_handle: bool,
}

#[cfg(windows)]
impl PipeStream {
    pub fn from_handle(handle: isize, owns: bool) -> Self {
        PipeStream {
            handle,
            owns_handle: owns,
        }
    }

    pub fn handle(&self) -> isize {
        self.handle
    }
}

#[cfg(windows)]
impl std::io::Read for PipeStream {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        let mut bytes_read: u32 = 0;
        let ok = unsafe {
            ReadFile(
                self.handle,
                buf.as_mut_ptr(),
                buf.len() as u32,
                &mut bytes_read,
                std::ptr::null_mut(),
            )
        };
        if ok == 0 {
            Err(std::io::Error::last_os_error())
        } else {
            Ok(bytes_read as usize)
        }
    }
}

#[cfg(windows)]
impl std::io::Write for PipeStream {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        let mut bytes_written: u32 = 0;
        let ok = unsafe {
            WriteFile(
                self.handle,
                buf.as_ptr(),
                buf.len() as u32,
                &mut bytes_written,
                std::ptr::null_mut(),
            )
        };
        if ok == 0 {
            Err(std::io::Error::last_os_error())
        } else {
            Ok(bytes_written as usize)
        }
    }

    fn flush(&mut self) -> std::io::Result<()> {
        let ok = unsafe { FlushFileBuffers(self.handle) };
        if ok == 0 {
            Err(std::io::Error::last_os_error())
        } else {
            Ok(())
        }
    }
}

#[cfg(windows)]
impl Drop for PipeStream {
    fn drop(&mut self) {
        if self.owns_handle && self.handle != -1 {
            unsafe {
                CloseHandle(self.handle);
            }
        }
    }
}
