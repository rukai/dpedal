use defmt::error;
use dpedal_config::{CONFIG_FLASH_RANGE, CONFIG_SINGLE_SIZE, Config, PICO_FLASH_SIZE};
use embassy_rp::{
    Peri,
    dma::Channel,
    flash::{Async, Flash},
    peripherals::FLASH,
};
use embassy_sync::{blocking_mutex::raw::CriticalSectionRawMutex, mutex::Mutex, watch::Watch};
use heapless::Vec;
use sequential_storage::{
    cache::NoCache,
    map::{MapConfig, MapStorage},
};
use static_cell::StaticCell;

pub static CONFIG: Mutex<CriticalSectionRawMutex, Option<Config>> = Mutex::new(None);
pub static CONFIG_UPDATED: Watch<CriticalSectionRawMutex, (), 2> = Watch::new();

const CONFIG_KEY: u8 = 0;

pub struct ConfigFlash {
    storage: MapStorage<u8, Flash<'static, FLASH, Async, PICO_FLASH_SIZE>, NoCache>,
    data_buffer: &'static mut [u8; CONFIG_SINGLE_SIZE + 8],
}

impl ConfigFlash {
    pub async fn new(p_flash: Peri<'static, FLASH>, dma: Peri<'static, impl Channel>) -> Self {
        static DATA_BUFFER: StaticCell<[u8; CONFIG_SINGLE_SIZE + 8]> = StaticCell::new();
        let data_buffer = DATA_BUFFER.init([0u8; CONFIG_SINGLE_SIZE + 8]);

        let flash = Flash::new(p_flash, dma);
        let mut config_flash = ConfigFlash {
            storage: MapStorage::new(
                flash,
                const { MapConfig::new(CONFIG_FLASH_RANGE) },
                NoCache::new(),
            ),
            data_buffer,
        };
        config_flash.load().await;
        config_flash
    }

    pub async fn load(&mut self) {
        if let Err(()) = self.load_inner().await {
            *CONFIG.lock().await = Some(Config::default());
            error!("Failed to load config from flash")
        }
        CONFIG_UPDATED.sender().send(());
    }

    async fn load_inner(&mut self) -> Result<(), ()> {
        let mut config_lock = CONFIG.lock().await;
        let bytes = self.load_config_bytes_from_flash().await?;
        *config_lock = Some(postcard::from_bytes::<Config>(&bytes).map_err(|_| ())?);
        Ok(())
    }

    pub async fn load_config_bytes_from_flash(
        &mut self,
    ) -> Result<Vec<u8, CONFIG_SINGLE_SIZE>, ()> {
        let result = self.storage
            .fetch_item::<&[u8]>(self.data_buffer, &CONFIG_KEY)
            .await
            .map_err(|_| ())?;
        match result {
            Some(bytes) => Vec::from_slice(bytes).map_err(|_| ()),
            None => {
                error!("No config found in flash");
                Err(())
            }
        }
    }

    pub fn check_valid_config(&self, bytes: &[u8]) -> Result<(), ()> {
        postcard::from_bytes::<Config>(bytes).map_err(|_| ())?;
        Ok(())
    }

    pub async fn store_config_bytes_to_flash_and_reload_config(
        &mut self,
        bytes: &Vec<u8, CONFIG_SINGLE_SIZE>,
    ) -> Result<(), ()> {
        if bytes.len() > CONFIG_SINGLE_SIZE {
            error!("config bytes too long {}", bytes.len());
            return Err(());
        }
        self.check_valid_config(bytes)?;

        self.storage
            .store_item::<&[u8]>(self.data_buffer, &CONFIG_KEY, &bytes.as_slice())
            .await
            .map_err(|_| ())?;

        self.load().await;
        Ok(())
    }
}
