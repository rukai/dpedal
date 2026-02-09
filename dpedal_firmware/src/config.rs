use arrayvec::ArrayVec;
use defmt::error;
use dpedal_config::{ArchivedConfig, CONFIG_OFFSET, CONFIG_SIZE, Config, PICO_FLASH_SIZE};
use embassy_rp::{
    Peri,
    flash::{Blocking, Flash},
    peripherals::FLASH,
};
use embassy_sync::{blocking_mutex::raw::CriticalSectionRawMutex, mutex::Mutex};
use rkyv::{rancor::Failure, util::Align};

pub static CONFIG: Mutex<CriticalSectionRawMutex, Option<Config>> = Mutex::new(None);

pub struct ConfigFlash {
    flash: Flash<'static, FLASH, Blocking, PICO_FLASH_SIZE>,
}

impl ConfigFlash {
    pub async fn new(p_flash: Peri<'static, FLASH>) -> Self {
        let mut flash = ConfigFlash {
            flash: Flash::new_blocking(p_flash),
        };
        flash.load().await;
        flash
    }

    pub async fn load(&mut self) {
        if let Err(()) = self.load_inner().await {
            *CONFIG.lock().await = Some(Config::default());
            error!("Failed to load config from flash")
        }
    }

    async fn load_inner(&mut self) -> Result<(), ()> {
        let bytes = self.load_config_bytes_from_flash()?;
        let archive = rkyv::api::low::access::<ArchivedConfig, Failure>(&bytes).map_err(|_| ())?;
        let mut config_lock = CONFIG.lock().await;
        *config_lock = Some(rkyv::api::low::deserialize::<_, Failure>(archive).map_err(|_| ())?);

        Ok(())
    }

    pub fn load_config_bytes_from_flash(&mut self) -> Result<Align<ArrayVec<u8, CONFIG_SIZE>>, ()> {
        // TODO: store in heap instead, apparently only 2kb of stack o.0
        let mut bytes = [0u8; CONFIG_SIZE];
        self.flash
            .blocking_read(CONFIG_OFFSET as u32, &mut bytes)
            .unwrap();
        let size = u32::from_be_bytes(bytes[..4].try_into().unwrap());

        let size = size as usize;
        if size > CONFIG_SIZE - 4 {
            error!("config bytes length prefix too long {}", size);
            return Err(());
        }

        Ok(Align(ArrayVec::from_iter(
            bytes[4..4 + size].iter().cloned(),
        )))
    }

    pub fn check_valid_config(&self, bytes: &[u8]) -> Result<(), ()> {
        let archive = rkyv::api::low::access::<ArchivedConfig, Failure>(bytes).map_err(|_| ())?;
        rkyv::api::low::deserialize::<_, Failure>(archive).map_err(|_| ())?;
        Ok(())
    }

    pub async fn load_config_bytes_to_flash_and_reload_config(
        &mut self,
        bytes: ArrayVec<u8, CONFIG_SIZE>,
    ) -> Result<(), ()> {
        let size = bytes.len();
        if size > CONFIG_SIZE {
            error!("config bytes length prefix too long {}", size);
            return Err(());
        }

        self.check_valid_config(&bytes)?;
        // TODO: Upstream this check, blocking_erase is not sound
        let block_aligned_size = (4 + size as u32).div_ceil(4096) * 4096;
        self.flash
            .blocking_erase(
                CONFIG_OFFSET as u32,
                CONFIG_OFFSET as u32 + block_aligned_size,
            )
            .unwrap();

        let mut final_bytes: ArrayVec<u8, CONFIG_SIZE> = ArrayVec::from_iter(size.to_be_bytes());
        final_bytes.extend(bytes);
        self.flash
            .blocking_write(CONFIG_OFFSET as u32, &final_bytes)
            .unwrap();

        defmt::info!("config of size {} written to flash", size.to_be_bytes());

        self.load().await;

        Ok(())
    }
}
