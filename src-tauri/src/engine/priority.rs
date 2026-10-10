use windows::Win32::System::Threading::AvSetMmThreadCharacteristicsW;
use windows::core::w;

#[derive(Debug, Default)]
pub struct Promotion {
    done: bool,
}

impl Promotion {
    pub fn once(&mut self) {
        if !self.done {
            promote_current_thread();
            self.done = true;
        }
    }
}

pub fn promote_current_thread() {
    let mut task = 0;
    unsafe { AvSetMmThreadCharacteristicsW(w!("Pro Audio"), &mut task) }.ok();
}
