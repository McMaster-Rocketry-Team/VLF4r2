use lora_phy::sx126x::{DeviceSel, Sx126xVariant};

pub struct E22;

impl Sx126xVariant for E22 {
    fn get_device_sel(&self) -> DeviceSel {
        DeviceSel::HighPowerPA
    }
    
    fn use_dio2_as_rfswitch(&self) -> bool {
        false
    }
}
