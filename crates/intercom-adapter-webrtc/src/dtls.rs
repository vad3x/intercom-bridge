//! Non-blocking DTLS 1.2 and RFC 5764 key export backed by a bounded Mbed TLS arena.

use alloc::boxed::Box;
use core::ffi::{c_int, c_void};
use core::sync::atomic::{AtomicU8, AtomicUsize, Ordering};

pub const DTLS_ARENA_BYTES: usize = 28_672;
const STATE_STORAGE_WORDS: usize = 768;
const DATAGRAM_MAX_BYTES: usize = 1_500;
#[cfg(not(test))]
const DTLS_ARENA_WORDS: usize = DTLS_ARENA_BYTES / core::mem::size_of::<u64>();
const HEAP_INITIALIZING: u8 = 1;
const HEAP_READY: u8 = 2;
static HEAP_STATE: AtomicU8 = AtomicU8::new(0);
static ACTIVE_ENGINES: AtomicUsize = AtomicUsize::new(0);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DtlsRole {
    Client,
    Server,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DtlsSrtpKeys {
    pub client_master_key: [u8; 16],
    pub server_master_key: [u8; 16],
    pub client_master_salt: [u8; 14],
    pub server_master_salt: [u8; 14],
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DtlsError {
    HeapInitializationFailed,
    HeapAlreadyInUse,
    StateStorageTooSmall,
    InitFailed(i32),
    InputFailed(i32),
    StepFailed(i32),
    DatagramTooLarge,
}

pub struct DtlsSrtpEngine {
    storage: Box<[u64; STATE_STORAGE_WORDS]>,
    heap_lease: bool,
    #[cfg(not(test))]
    _heap_arena: Box<[u64; DTLS_ARENA_WORDS]>,
    local_cert_sha256: [u8; 32],
}

impl DtlsSrtpEngine {
    pub fn new(role: DtlsRole, seed: [u8; 32]) -> Result<Self, DtlsError> {
        #[cfg(test)]
        let (heap_ptr, heap_len) = heap_buffer();
        #[cfg(not(test))]
        let mut heap_arena = Box::new([0; DTLS_ARENA_WORDS]);
        #[cfg(not(test))]
        let heap_ptr = heap_arena.as_mut_ptr().cast::<u8>();
        #[cfg(not(test))]
        let heap_len = DTLS_ARENA_BYTES;

        let owns_heap = initialize_mbedtls_heap(heap_ptr, heap_len)?;
        let state_size = unsafe { intercom_dtls_state_size() };
        if state_size > core::mem::size_of::<[u64; STATE_STORAGE_WORDS]>() {
            free_new_heap(owns_heap);
            return Err(DtlsError::StateStorageTooSmall);
        }

        let mut engine = Self {
            storage: Box::new([0; STATE_STORAGE_WORDS]),
            heap_lease: false,
            #[cfg(not(test))]
            _heap_arena: heap_arena,
            local_cert_sha256: [0; 32],
        };
        let role = match role {
            DtlsRole::Client => 0,
            DtlsRole::Server => 1,
        };
        let result = unsafe {
            intercom_dtls_init(
                engine.storage.as_mut_ptr().cast::<c_void>(),
                core::mem::size_of::<[u64; STATE_STORAGE_WORDS]>(),
                role,
                seed.as_ptr(),
            )
        };
        if result != 0 {
            free_new_heap(owns_heap);
            return Err(DtlsError::InitFailed(result));
        }
        let result = unsafe {
            intercom_dtls_local_fingerprint(
                engine.storage.as_ptr().cast_mut().cast::<c_void>(),
                engine.local_cert_sha256.as_mut_ptr(),
            )
        };
        if result == 0 {
            unsafe { intercom_dtls_free(engine.storage.as_mut_ptr().cast::<c_void>()) };
            free_new_heap(owns_heap);
            return Err(DtlsError::InitFailed(-1));
        }
        ACTIVE_ENGINES.fetch_add(1, Ordering::AcqRel);
        engine.heap_lease = true;
        Ok(engine)
    }

    pub fn step(&mut self, now_ms: u64) -> Result<bool, DtlsError> {
        let result =
            unsafe { intercom_dtls_step(self.storage.as_mut_ptr().cast::<c_void>(), now_ms) };
        match result {
            0 => Ok(true),
            1 => Ok(false),
            error => Err(DtlsError::StepFailed(error)),
        }
    }

    pub fn push_inbound_datagram(&mut self, datagram: &[u8]) -> Result<(), DtlsError> {
        if datagram.is_empty() || datagram.len() > DATAGRAM_MAX_BYTES {
            return Err(DtlsError::DatagramTooLarge);
        }
        let result = unsafe {
            intercom_dtls_push_input(
                self.storage.as_mut_ptr().cast::<c_void>(),
                datagram.as_ptr(),
                datagram.len(),
            )
        };
        if result == 0 {
            Ok(())
        } else {
            Err(DtlsError::InputFailed(result))
        }
    }

    pub fn set_peer_transport_id(&mut self, id: &[u8]) -> Result<(), DtlsError> {
        let result = unsafe {
            intercom_dtls_set_peer_transport_id(
                self.storage.as_mut_ptr().cast::<c_void>(),
                id.as_ptr(),
                id.len(),
            )
        };
        if result == 0 {
            Ok(())
        } else {
            Err(DtlsError::InputFailed(result))
        }
    }

    pub fn pop_outbound_datagram(&mut self, output: &mut [u8]) -> Option<usize> {
        let length = unsafe {
            intercom_dtls_pop_output(
                self.storage.as_mut_ptr().cast::<c_void>(),
                output.as_mut_ptr(),
                output.len(),
            )
        };
        (length != 0).then_some(length)
    }

    pub fn exported_srtp_keys(&self) -> Option<DtlsSrtpKeys> {
        let mut key_material = [0u8; 60];
        let ready = unsafe {
            intercom_dtls_exported_keys(
                self.storage.as_ptr().cast_mut().cast::<c_void>(),
                key_material.as_mut_ptr(),
            )
        };
        if ready == 0 {
            return None;
        }
        let mut keys = DtlsSrtpKeys {
            client_master_key: [0; 16],
            server_master_key: [0; 16],
            client_master_salt: [0; 14],
            server_master_salt: [0; 14],
        };
        keys.client_master_key.copy_from_slice(&key_material[..16]);
        keys.server_master_key
            .copy_from_slice(&key_material[16..32]);
        keys.client_master_salt
            .copy_from_slice(&key_material[32..46]);
        keys.server_master_salt
            .copy_from_slice(&key_material[46..60]);
        Some(keys)
    }

    pub const fn local_cert_sha256(&self) -> [u8; 32] {
        self.local_cert_sha256
    }

    pub fn peer_cert_sha256(&self) -> Option<[u8; 32]> {
        let mut fingerprint = [0u8; 32];
        let ready = unsafe {
            intercom_dtls_peer_fingerprint(
                self.storage.as_ptr().cast_mut().cast::<c_void>(),
                fingerprint.as_mut_ptr(),
            )
        };
        (ready != 0).then_some(fingerprint)
    }
}

impl Drop for DtlsSrtpEngine {
    fn drop(&mut self) {
        unsafe { intercom_dtls_free(self.storage.as_mut_ptr().cast::<c_void>()) };
        if self.heap_lease {
            release_mbedtls_heap();
        }
    }
}

fn initialize_mbedtls_heap(buffer: *mut u8, length: usize) -> Result<bool, DtlsError> {
    loop {
        match HEAP_STATE.load(Ordering::Acquire) {
            HEAP_READY => {
                #[cfg(test)]
                return Ok(false);
                #[cfg(not(test))]
                return Err(DtlsError::HeapAlreadyInUse);
            }
            HEAP_INITIALIZING => core::hint::spin_loop(),
            0 => {
                if HEAP_STATE
                    .compare_exchange(0, HEAP_INITIALIZING, Ordering::AcqRel, Ordering::Acquire)
                    .is_err()
                {
                    continue;
                }
                let result = unsafe { intercom_dtls_heap_init(buffer, length) };
                if result == 0 {
                    HEAP_STATE.store(HEAP_READY, Ordering::Release);
                    return Ok(true);
                }
                HEAP_STATE.store(0, Ordering::Release);
                return Err(DtlsError::HeapInitializationFailed);
            }
            _ => return Err(DtlsError::HeapInitializationFailed),
        }
    }
}

fn release_mbedtls_heap() {
    if ACTIVE_ENGINES.fetch_sub(1, Ordering::AcqRel) == 1 {
        unsafe { intercom_dtls_heap_free() };
        HEAP_STATE.store(0, Ordering::Release);
    }
}

fn free_new_heap(owns_heap: bool) {
    if owns_heap {
        unsafe { intercom_dtls_heap_free() };
        HEAP_STATE.store(0, Ordering::Release);
    }
}

#[cfg(test)]
fn heap_buffer() -> (*mut u8, usize) {
    static mut HOST_MBEDTLS_HEAP: [u64; 8_192] = [0; 8_192];
    let buffer = core::ptr::addr_of_mut!(HOST_MBEDTLS_HEAP).cast::<u8>();
    (buffer, 8_192 * core::mem::size_of::<u64>())
}

unsafe extern "C" {
    fn intercom_dtls_heap_init(buffer: *mut u8, length: usize) -> c_int;
    fn intercom_dtls_heap_free();
    fn intercom_dtls_state_size() -> usize;
    fn intercom_dtls_init(
        storage: *mut c_void,
        storage_length: usize,
        role: c_int,
        seed: *const u8,
    ) -> c_int;
    fn intercom_dtls_free(storage: *mut c_void);
    fn intercom_dtls_push_input(storage: *mut c_void, data: *const u8, length: usize) -> c_int;
    fn intercom_dtls_set_peer_transport_id(
        storage: *mut c_void,
        data: *const u8,
        length: usize,
    ) -> c_int;
    fn intercom_dtls_step(storage: *mut c_void, now_ms: u64) -> c_int;
    fn intercom_dtls_pop_output(storage: *mut c_void, output: *mut u8, capacity: usize) -> usize;
    fn intercom_dtls_exported_keys(storage: *mut c_void, output: *mut u8) -> c_int;
    fn intercom_dtls_local_fingerprint(storage: *mut c_void, output: *mut u8) -> c_int;
    fn intercom_dtls_peer_fingerprint(storage: *mut c_void, output: *mut u8) -> c_int;
}

#[cfg(test)]
mod tests {
    use super::{DtlsRole, DtlsSrtpEngine};
    use core::sync::atomic::Ordering;
    use std::sync::Mutex;

    static TEST_LOCK: Mutex<()> = Mutex::new(());

    #[test]
    fn dtls_1_2_client_and_server_complete_handshake_and_export_matching_srtp_keys() {
        // Arrange
        let _guard = TEST_LOCK
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let mut server = DtlsSrtpEngine::new(DtlsRole::Server, [0x42; 32]).expect("server init");
        let mut client = DtlsSrtpEngine::new(DtlsRole::Client, [0x99; 32]).expect("client init");
        server
            .set_peer_transport_id(b"verified-ice-peer")
            .expect("server peer id");
        let client_peer_id_result = client.set_peer_transport_id(b"client-peer-id");
        let mut wire_buffer = [0u8; 1500];
        let mut client_done = false;
        let mut server_done = false;

        // Act
        for step in 0..64 {
            client_done = client.step(step * 100).expect("client step");
            while let Some(length) = client.pop_outbound_datagram(&mut wire_buffer) {
                server
                    .push_inbound_datagram(&wire_buffer[..length])
                    .expect("server input");
            }
            server_done = server.step(step * 100).expect("server step");
            while let Some(length) = server.pop_outbound_datagram(&mut wire_buffer) {
                client
                    .push_inbound_datagram(&wire_buffer[..length])
                    .expect("client input");
            }
            if client_done && server_done {
                break;
            }
        }
        assert!(
            client_done && server_done,
            "handshake incomplete c={client_done} s={server_done}"
        );
        assert!(matches!(
            client_peer_id_result,
            Err(super::DtlsError::InputFailed(_))
        ));
        let client_keys = client.exported_srtp_keys().expect("client keys");
        let server_keys = server.exported_srtp_keys().expect("server keys");

        // Assert
        assert_eq!(client_keys, server_keys);
        assert_eq!(client.peer_cert_sha256(), Some(server.local_cert_sha256()));
        assert_eq!(server.peer_cert_sha256(), Some(client.local_cert_sha256()));
    }

    #[test]
    fn opaque_storage_covers_c_state() {
        // Arrange
        let c_state_bytes = unsafe { super::intercom_dtls_state_size() };
        let rust_storage_bytes = core::mem::size_of::<[u64; super::STATE_STORAGE_WORDS]>();

        // Act
        let has_capacity = c_state_bytes <= rust_storage_bytes;

        // Assert
        assert!(
            has_capacity,
            "C state {c_state_bytes} exceeds {rust_storage_bytes} bytes"
        );
    }

    #[test]
    fn allocator_arena_is_released_and_can_be_reinitialized() {
        // Arrange
        let _guard = TEST_LOCK
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let (heap_ptr, _) = super::heap_buffer();

        // Act
        let failed_init = super::initialize_mbedtls_heap(heap_ptr, 128);
        let reset_after_failure = super::HEAP_STATE.load(Ordering::Acquire);
        let first = DtlsSrtpEngine::new(DtlsRole::Client, [0x21; 32]);
        let first = first.expect("engine after failed arena initialization");
        drop(first);
        let mut second = DtlsSrtpEngine::new(DtlsRole::Client, [0x22; 32]).expect("second engine");

        // Assert
        assert_eq!(failed_init, Err(super::DtlsError::HeapInitializationFailed));
        assert_eq!(reset_after_failure, 0);
        assert!(!second.step(0).expect("step after arena reinitialization"));
    }
}
