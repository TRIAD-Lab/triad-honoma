use crate::trvm::accounts::RawAccount;
use crate::sdk::pubkey::Pubkey;
use std::ptr::NonNull;

/// Safe view over a RawAccount with generation validation.
#[derive(Debug, Clone)]
pub struct AccountView {
    raw: RawAccount,
}

impl AccountView {
    pub fn from_raw(raw: &RawAccount) -> Result<Self, String> {
        raw.check_safety()?;
        Ok(Self { raw: raw.clone() })
    }

    #[inline]
    fn validate(&self) -> Result<(), String> {
        self.raw.check_safety()
    }

    pub fn lamports(&self) -> Result<u64, String> {
        self.validate()?;
        unsafe { Ok(*self.raw.lamports.as_ptr()) }
    }

    pub fn set_lamports(&mut self, lamports: u64) -> Result<(), String> {
        self.validate()?;
        unsafe {
            *self.raw.lamports.as_ptr() = lamports;
        }
        Ok(())
    }

    pub fn data(&self) -> Result<&[u8], String> {
        self.validate()?;
        unsafe { Ok(std::slice::from_raw_parts(self.raw.data.as_ptr(), self.raw.data_len)) }
    }

    pub fn data_mut(&mut self) -> Result<&mut [u8], String> {
        self.validate()?;
        unsafe { Ok(std::slice::from_raw_parts_mut(self.raw.data.as_ptr(), self.raw.data_len)) }
    }

    pub fn owner(&self) -> Result<&Pubkey, String> {
        self.validate()?;
        unsafe { Ok(self.raw.owner.as_ref()) }
    }

    pub fn set_owner(&mut self, owner: Pubkey) -> Result<(), String> {
        self.validate()?;
        unsafe {
            *self.raw.owner.as_ptr() = owner;
        }
        Ok(())
    }

    pub fn executable(&self) -> Result<bool, String> {
        self.validate()?;
        unsafe { Ok(*self.raw.executable.as_ptr()) }
    }

    pub fn set_executable(&mut self, executable: bool) -> Result<(), String> {
        self.validate()?;
        unsafe {
            *self.raw.executable.as_ptr() = executable;
        }
        Ok(())
    }

    pub fn rent_epoch(&self) -> Result<u64, String> {
        self.validate()?;
        unsafe { Ok(*self.raw.rent_epoch.as_ptr()) }
    }

    pub fn set_rent_epoch(&mut self, rent_epoch: u64) -> Result<(), String> {
        self.validate()?;
        unsafe {
            *self.raw.rent_epoch.as_ptr() = rent_epoch;
        }
        Ok(())
    }

    /// Generation length setter for realloc; caller must ensure capacity is valid.
    pub fn set_data_len(&mut self, new_len: usize) -> Result<(), String> {
        self.validate()?;
        self.raw.data_len = new_len;
        Ok(())
    }
}

unsafe impl Send for AccountView {}
unsafe impl Sync for AccountView {}
