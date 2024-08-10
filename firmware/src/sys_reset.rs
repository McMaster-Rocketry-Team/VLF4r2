use firmware_common::driver::sys_reset::SysReset as CommonSysReset;

#[derive(Clone, Copy)]
pub struct SysReset {}

impl SysReset {
    pub fn new() -> Self {
        Self {}
    }
}

impl CommonSysReset for SysReset {
    fn reset(&self) -> ! {
        cortex_m::peripheral::SCB::sys_reset()
    }
}
