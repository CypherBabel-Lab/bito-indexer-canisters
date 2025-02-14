use std::cell::RefCell;

use ic_stable_structures::memory_manager::{MemoryId, MemoryManager, VirtualMemory};
use ic_stable_structures::DefaultMemoryImpl;

pub const STATE_MEMORY_ID: MemoryId = MemoryId::new(0);
pub const NONCE_MANAGER_MEMORY_ID: MemoryId = MemoryId::new(1);
pub const CREATED_HISTORIES_ID: MemoryId = MemoryId::new(2);
pub const PENDING_BITCOIN_HISTORIES_ID: MemoryId = MemoryId::new(3);
pub const PENDING_HISTORIES_ID: MemoryId = MemoryId::new(4);
pub const HISTORIES_ID: MemoryId = MemoryId::new(5);
pub const CACHED_EVENT_TEMP_MEMORY_ID: MemoryId = MemoryId::new(6);

pub const FEE_COLLECTOR_ID: MemoryId = MemoryId::new(7);

pub const HISTORY_INDEX_BITCOIN_ID: MemoryId = MemoryId::new(8);
pub const HISTORY_INDEX_EVM_ID: MemoryId = MemoryId::new(9);
pub const HISTORY_INDEX_ICP_ID: MemoryId = MemoryId::new(10);
pub const HISTORY_INDEX_SUI_ID: MemoryId = MemoryId::new(11);

pub type VMemory = VirtualMemory<DefaultMemoryImpl>;

thread_local! {
  pub static MEMORY_MANAGER: RefCell<MemoryManager<DefaultMemoryImpl>> = RefCell::new(MemoryManager::init(DefaultMemoryImpl::default()));
}

/// GetVirtualMemory returns the virtual memory for the current thread.
pub fn get_virtual_memory(id: MemoryId) -> VMemory {
    MEMORY_MANAGER.with(|m| m.borrow().get(id))
}