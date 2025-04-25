use std::cell::RefCell;

use ic_stable_structures::memory_manager::{MemoryId, MemoryManager, VirtualMemory};
use ic_stable_structures::DefaultMemoryImpl;

pub const CONFIG_MEMORY_ID: MemoryId = MemoryId::new(0);
pub const HEIGHT_TO_BLOCK_HEADER_MEMORY_ID: MemoryId = MemoryId::new(1);
pub const HEIGHT_TO_LAST_SEQUENCE_NUMBER_MEMORY_ID: MemoryId = MemoryId::new(2);
pub const HOME_INSCRIPTIONS_MEMORY_ID: MemoryId = MemoryId::new(3);
pub const INSCRIPTION_ID_TO_SEQUENCE_NUMBER_MEMORY_ID: MemoryId = MemoryId::new(4);
pub const INSCRIPTION_NUMBER_TO_SEQUENCE_NUMBER_MEMORY_ID: MemoryId = MemoryId::new(5);
pub const OUTPOINT_TO_RUNE_BALANCES_MEMORY_ID: MemoryId = MemoryId::new(6);
pub const OUTPOINT_TO_HEIGHT_MEMORY_ID: MemoryId = MemoryId::new(7);
pub const OUTPOINT_TO_UTXO_ENTRY_MEMORY_ID: MemoryId = MemoryId::new(8);
pub const RUNE_ID_TO_RUNE_ENTRY_MEMORY_ID: MemoryId = MemoryId::new(9);
pub const RUNE_TO_RUNE_ID_MEMORY_ID: MemoryId = MemoryId::new(10);
pub const SAT_TO_SATPOINT_MEMORY_ID: MemoryId = MemoryId::new(11);
pub const SEQUENCE_NUMBER_TO_INSCRIPTION_ENTRY_MEMORY_ID: MemoryId = MemoryId::new(12);
pub const SEQUENCE_NUMBER_TO_RUNE_ID_MEMORY_ID: MemoryId = MemoryId::new(13);
pub const SEQUENCE_NUMBER_TO_SATPOINT_MEMORY_ID: MemoryId = MemoryId::new(14);
pub const STATISTIC_TO_COUNT_MEMORY_ID: MemoryId = MemoryId::new(15);
pub const TRANSACTION_ID_TO_RUNE_MEMORY_ID: MemoryId = MemoryId::new(16);
pub const TRANSACTION_ID_TO_TRANSACTION_MEMORY_ID: MemoryId = MemoryId::new(17);
// pub const WRITE_TRANSACTION_STARTING_BLOCK_COUNT_TO_TIMESTAMP_MEMORY_ID: MemoryId = MemoryId::new(18);
// pub const HEIGHT_TO_EVENTS_MEMORY_ID: MemoryId = MemoryId::new(19);
// multimap memories
pub const SAT_TO_SEQUENCE_NUMBERS_MEMORY_ID: MemoryId = MemoryId::new(20);
pub const SEQUENCE_NUMBER_TO_CHILDRENS_MEMORY_ID: MemoryId = MemoryId::new(21);
pub const SCRIPT_PUBKEY_TO_OUTPOINTS_MEMORY_ID: MemoryId = MemoryId::new(22);


pub const HEIGHT_TO_CHANGE_RECORD_RUNE_MEMORY_ID: MemoryId = MemoryId::new(23);
pub const HEIGHT_TO_STATISTIC_RUNES_MEMORY_ID: MemoryId = MemoryId::new(24);
pub const HEIGHT_TO_STATISTIC_RESERVED_RUNES_MEMORY_ID: MemoryId = MemoryId::new(25);
pub type VMemory = VirtualMemory<DefaultMemoryImpl>;

thread_local! {
  pub static MEMORY_MANAGER: RefCell<MemoryManager<DefaultMemoryImpl>> = RefCell::new(MemoryManager::init(DefaultMemoryImpl::default()));
}

/// GetVirtualMemory returns the virtual memory for the current thread.
pub fn get_virtual_memory(id: MemoryId) -> VMemory {
    MEMORY_MANAGER.with(|m| m.borrow().get(id))
}