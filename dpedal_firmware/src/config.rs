use defmt::error;
use dpedal_config::{
    CONFIG_FLASH_RANGE, ConfigKey, MAX_PROFILES, META_SERIALIZED_SIZE, Meta, PICO_FLASH_SIZE,
    PROFILE_SERIALIZED_SIZE, Profile,
};
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

pub static CONFIG_UPDATED: Watch<CriticalSectionRawMutex, (), 2> = Watch::new();

// Data buffer large enough for the largest item (Profile) plus key and overhead.
const DATA_BUFFER_SIZE: usize = PROFILE_SERIALIZED_SIZE + 32;

pub struct ConfigFlash {
    storage: MapStorage<u8, Flash<'static, FLASH, Async, PICO_FLASH_SIZE>, NoCache>,
    data_buffer: &'static mut [u8; DATA_BUFFER_SIZE],
}

impl ConfigFlash {
    pub async fn new(
        p_flash: Peri<'static, FLASH>,
        dma: Peri<'static, impl Channel>,
    ) -> &'static Mutex<CriticalSectionRawMutex, ConfigFlash> {
        static DATA_BUFFER: StaticCell<[u8; DATA_BUFFER_SIZE]> = StaticCell::new();
        let data_buffer = DATA_BUFFER.init([0u8; DATA_BUFFER_SIZE]);

        let flash = Flash::new(p_flash, dma);
        let config_flash = ConfigFlash {
            storage: MapStorage::new(
                flash,
                const { MapConfig::new(CONFIG_FLASH_RANGE) },
                NoCache::new(),
            ),
            data_buffer,
        };

        static SHARED: StaticCell<Mutex<CriticalSectionRawMutex, ConfigFlash>> = StaticCell::new();
        SHARED.init(Mutex::new(config_flash))
    }

    pub async fn load_meta_bytes(&mut self) -> Result<Vec<u8, META_SERIALIZED_SIZE>, ()> {
        let key = ConfigKey::Meta.key();
        let result = self
            .storage
            .fetch_item::<&[u8]>(self.data_buffer, &key)
            .await
            .map_err(|_| ())?;
        match result {
            Some(bytes) => Vec::from_slice(bytes).map_err(|_| ()),
            None => {
                error!("No meta found in flash");
                Err(())
            }
        }
    }

    pub async fn load_meta(&mut self) -> Meta {
        match self.load_meta_bytes().await {
            Ok(bytes) => postcard::from_bytes::<Meta>(&bytes).unwrap_or_default(),
            Err(()) => Meta::default(),
        }
    }

    pub async fn load_profile_bytes(
        &mut self,
        index: u8,
    ) -> Result<Vec<u8, PROFILE_SERIALIZED_SIZE>, ()> {
        let key = ConfigKey::Profile(index).key();
        let result = self
            .storage
            .fetch_item::<&[u8]>(self.data_buffer, &key)
            .await
            .map_err(|_| ())?;
        match result {
            Some(bytes) => Vec::from_slice(bytes).map_err(|_| ()),
            None => {
                error!("No profile {} found in flash", index);
                Err(())
            }
        }
    }

    pub async fn load_profile(&mut self, index: u8) -> Profile {
        match self.load_profile_bytes(index).await {
            Ok(bytes) => postcard::from_bytes::<Profile>(&bytes).unwrap_or_default(),
            Err(()) => Profile::default(),
        }
    }

    pub async fn get_profile_count(&mut self) -> u8 {
        let key = ConfigKey::ProfileCount.key();
        let result = self
            .storage
            .fetch_item::<u8>(self.data_buffer, &key)
            .await
            .map_err(|_| ());
        match result {
            Ok(Some(bytes)) => bytes,
            _ => 0,
        }
    }

    /// Store new meta bytes. As a side effect, resets ProfileCount to 0
    /// (old Profile entries remain in flash but become unreachable until new profiles are added).
    pub async fn store_meta(&mut self, bytes: &[u8]) -> Result<(), ()> {
        postcard::from_bytes::<Meta>(bytes).map_err(|_| ())?;

        self.storage
            .store_item::<&[u8]>(self.data_buffer, &ConfigKey::Meta.key(), &bytes)
            .await
            .map_err(|_| ())?;

        // Reset profile count; old Profile(i) entries are orphaned in flash
        // and will be skipped since the count is now 0.
        // TODO: dont orphan entries, its not too bad since they will get cleaned up next time we readd a profile with that entry,
        //       but we should be proactive about it for the sake of wear leveling
        self.storage
            .store_item::<&[u8]>(
                self.data_buffer,
                &ConfigKey::ProfileCount.key(),
                &[0u8].as_slice(),
            )
            .await
            .map_err(|_| ())
    }

    /// Append a new profile at the current profile count index, then increment the count.
    pub async fn add_profile(&mut self, bytes: &[u8]) -> Result<(), ()> {
        postcard::from_bytes::<Profile>(bytes).map_err(|_| ())?;

        let count = self.get_profile_count().await;
        if count >= MAX_PROFILES as u8 {
            return Err(());
        }

        self.storage
            .store_item::<&[u8]>(self.data_buffer, &ConfigKey::Profile(count).key(), &bytes)
            .await
            .map_err(|_| ())?;

        self.storage
            .store_item::<u8>(
                self.data_buffer,
                &ConfigKey::ProfileCount.key(),
                &(count + 1),
            )
            .await
            .map_err(|_| ())
    }
}
