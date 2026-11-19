//! Non-blocking DTLS 1.2 and RFC 5764 key export backed by a bounded Mbed TLS arena.

use core::ffi::{c_int, c_void};
use core::ops::{Deref, DerefMut};
use core::sync::atomic::{AtomicU8, AtomicUsize, Ordering};

use embassy_sync::blocking_mutex::raw::CriticalSectionRawMutex;
use embassy_sync::mutex::{Mutex, MutexGuard};

pub const DTLS_ARENA_BYTES: usize = 26 * 1024;
const STATE_STORAGE_WORDS: usize = 1_920;
const DATAGRAM_MAX_BYTES: usize = 1_500;
const DTLS_CERT_DER_MAX: usize = 1_024;
const DTLS_KEY_DER_MAX: usize = 256;
const DTLS_ARENA_WORDS: usize = DTLS_ARENA_BYTES / core::mem::size_of::<u64>();
const HEAP_INITIALIZING: u8 = 1;
const HEAP_READY: u8 = 2;
static HEAP_STATE: AtomicU8 = AtomicU8::new(0);
static ACTIVE_ENGINES: AtomicUsize = AtomicUsize::new(0);
#[cfg(test)]
pub(crate) static DTLS_TEST_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

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

/// Fixed storage for one active DTLS-SRTP engine.
pub struct DtlsSrtpResources {
    storage: [u64; STATE_STORAGE_WORDS],
    heap_arena: [u64; DTLS_ARENA_WORDS],
}

impl DtlsSrtpResources {
    pub const fn new() -> Self {
        Self {
            storage: [0; STATE_STORAGE_WORDS],
            heap_arena: [0; DTLS_ARENA_WORDS],
        }
    }
}

/// Shared storage for offer fingerprinting and the active media session.
///
/// The offer handler borrows this synchronously to generate the local certificate
/// fingerprint. The media task borrows it for the lifetime of the DTLS session.
pub struct DtlsSrtpResourceCell<'a> {
    lock: Mutex<CriticalSectionRawMutex, &'a mut DtlsSrtpResources>,
}

impl<'a> DtlsSrtpResourceCell<'a> {
    pub fn new(resources: &'a mut DtlsSrtpResources) -> Self {
        Self {
            lock: Mutex::new(resources),
        }
    }

    pub fn try_borrow_mut(&self) -> Option<DtlsSrtpResourceGuard<'_, 'a>> {
        Some(DtlsSrtpResourceGuard {
            lock: self.lock.try_lock().ok()?,
        })
    }

    pub async fn borrow_mut(&self) -> DtlsSrtpResourceGuard<'_, 'a> {
        DtlsSrtpResourceGuard {
            lock: self.lock.lock().await,
        }
    }
}

pub struct DtlsSrtpResourceGuard<'lock, 'resource> {
    lock: MutexGuard<'lock, CriticalSectionRawMutex, &'resource mut DtlsSrtpResources>,
}

impl Deref for DtlsSrtpResourceGuard<'_, '_> {
    type Target = DtlsSrtpResources;

    fn deref(&self) -> &Self::Target {
        &**self.lock
    }
}

impl DerefMut for DtlsSrtpResourceGuard<'_, '_> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut **self.lock
    }
}

/// Pre-generated ECDSA P-256 keypair, self-signed X.509 certificate DER, and SHA-256 fingerprint.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DtlsIdentity {
    cert_der: [u8; DTLS_CERT_DER_MAX],
    cert_der_len: usize,
    key_der: [u8; DTLS_KEY_DER_MAX],
    key_der_len: usize,
    fingerprint_sha256: [u8; 32],
}

impl DtlsIdentity {
    pub fn generate(
        seed: [u8; 32],
        #[cfg_attr(test, allow(unused_variables))] resources: &mut DtlsSrtpResources,
    ) -> Result<Self, DtlsError> {
        #[cfg(test)]
        let (heap_ptr, heap_len) = heap_buffer();
        #[cfg(not(test))]
        let heap_ptr = resources.heap_arena.as_mut_ptr().cast::<u8>();
        #[cfg(not(test))]
        let heap_len = DTLS_ARENA_BYTES;

        let owns_heap = initialize_mbedtls_heap(heap_ptr, heap_len)?;
        let mut identity = Self {
            cert_der: [0; DTLS_CERT_DER_MAX],
            cert_der_len: 0,
            key_der: [0; DTLS_KEY_DER_MAX],
            key_der_len: 0,
            fingerprint_sha256: [0; 32],
        };
        let result = unsafe {
            intercom_dtls_generate_identity(
                seed.as_ptr(),
                identity.cert_der.as_mut_ptr(),
                identity.cert_der.len(),
                &mut identity.cert_der_len,
                identity.key_der.as_mut_ptr(),
                identity.key_der.len(),
                &mut identity.key_der_len,
                identity.fingerprint_sha256.as_mut_ptr(),
            )
        };
        free_new_heap(owns_heap);
        if result == 0 {
            Ok(identity)
        } else {
            Err(DtlsError::InitFailed(result))
        }
    }

    pub fn cert_der(&self) -> &[u8] {
        &self.cert_der[..self.cert_der_len]
    }

    pub fn key_der(&self) -> &[u8] {
        &self.key_der[..self.key_der_len]
    }

    pub const fn fingerprint_sha256(&self) -> [u8; 32] {
        self.fingerprint_sha256
    }
}

pub struct DtlsSrtpEngine<'a> {
    storage: &'a mut [u64; STATE_STORAGE_WORDS],
    heap_lease: bool,
    _heap_arena: &'a mut [u64; DTLS_ARENA_WORDS],
    local_cert_sha256: [u8; 32],
}

impl<'a> DtlsSrtpEngine<'a> {
    pub fn new(
        role: DtlsRole,
        seed: [u8; 32],
        resources: &'a mut DtlsSrtpResources,
    ) -> Result<Self, DtlsError> {
        Self::new_inner(role, seed, None, resources)
    }

    pub fn new_with_identity(
        role: DtlsRole,
        seed: [u8; 32],
        identity: &DtlsIdentity,
        resources: &'a mut DtlsSrtpResources,
    ) -> Result<Self, DtlsError> {
        Self::new_inner(role, seed, Some(identity), resources)
    }

    fn new_inner(
        role: DtlsRole,
        seed: [u8; 32],
        identity: Option<&DtlsIdentity>,
        resources: &'a mut DtlsSrtpResources,
    ) -> Result<Self, DtlsError> {
        #[cfg(test)]
        let (heap_ptr, heap_len) = heap_buffer();
        #[cfg(not(test))]
        let heap_ptr = resources.heap_arena.as_mut_ptr().cast::<u8>();
        #[cfg(not(test))]
        let heap_len = DTLS_ARENA_BYTES;

        let owns_heap = initialize_mbedtls_heap(heap_ptr, heap_len)?;
        let state_size = unsafe { intercom_dtls_state_size() };
        if state_size > core::mem::size_of::<[u64; STATE_STORAGE_WORDS]>() {
            free_new_heap(owns_heap);
            return Err(DtlsError::StateStorageTooSmall);
        }

        let mut engine = Self {
            storage: &mut resources.storage,
            heap_lease: false,
            _heap_arena: &mut resources.heap_arena,
            local_cert_sha256: [0; 32],
        };
        let role = match role {
            DtlsRole::Client => 0,
            DtlsRole::Server => 1,
        };
        let result = match identity {
            Some(identity) => unsafe {
                intercom_dtls_init_with_identity(
                    engine.storage.as_mut_ptr().cast::<c_void>(),
                    core::mem::size_of::<[u64; STATE_STORAGE_WORDS]>(),
                    role,
                    seed.as_ptr(),
                    identity.cert_der().as_ptr(),
                    identity.cert_der().len(),
                    identity.key_der().as_ptr(),
                    identity.key_der().len(),
                    identity.fingerprint_sha256.as_ptr(),
                )
            },
            None => unsafe {
                intercom_dtls_init(
                    engine.storage.as_mut_ptr().cast::<c_void>(),
                    core::mem::size_of::<[u64; STATE_STORAGE_WORDS]>(),
                    role,
                    seed.as_ptr(),
                )
            },
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

    #[cfg(test)]
    pub(crate) fn set_mtu(&mut self, mtu: u16) {
        unsafe { intercom_dtls_set_mtu(self.storage.as_mut_ptr().cast::<c_void>(), mtu) };
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

    pub fn export_diagnostic(&self) -> (bool, i32, i32, i32) {
        let (mut called, mut export_type, mut prf, mut error) = (0, 0, 0, 0);
        unsafe {
            intercom_dtls_export_diagnostic(
                self.storage.as_ptr().cast_mut().cast::<c_void>(),
                &mut called,
                &mut export_type,
                &mut prf,
                &mut error,
            )
        };
        (called != 0, export_type, prf, error)
    }

    pub fn debug_diagnostic(&self) -> (i32, i32, [u8; 128], usize) {
        let (mut level, mut line, mut message) = (0, 0, [0; 128]);
        let length = unsafe {
            intercom_dtls_debug_diagnostic(
                self.storage.as_ptr().cast_mut().cast::<c_void>(),
                &mut level,
                &mut line,
                message.as_mut_ptr(),
                message.len(),
            )
        };
        (level, line, message, length)
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

impl Drop for DtlsSrtpEngine<'_> {
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
    // Host handshake tests keep both client and server engines alive concurrently.
    static mut HOST_MBEDTLS_HEAP: [u64; 8_192] = [0; 8_192];
    let buffer = core::ptr::addr_of_mut!(HOST_MBEDTLS_HEAP).cast::<u8>();
    (buffer, 8_192 * core::mem::size_of::<u64>())
}

unsafe extern "C" {
    fn intercom_dtls_heap_init(buffer: *mut u8, length: usize) -> c_int;
    fn intercom_dtls_heap_free();
    fn intercom_dtls_state_size() -> usize;
    fn intercom_dtls_generate_identity(
        seed: *const u8,
        cert_der_out: *mut u8,
        cert_der_capacity: usize,
        cert_der_len_out: *mut usize,
        key_der_out: *mut u8,
        key_der_capacity: usize,
        key_der_len_out: *mut usize,
        fingerprint_out: *mut u8,
    ) -> c_int;
    fn intercom_dtls_init(
        storage: *mut c_void,
        storage_length: usize,
        role: c_int,
        seed: *const u8,
    ) -> c_int;
    fn intercom_dtls_init_with_identity(
        storage: *mut c_void,
        storage_length: usize,
        role: c_int,
        seed: *const u8,
        cert_der: *const u8,
        cert_der_len: usize,
        key_der: *const u8,
        key_der_len: usize,
        fingerprint: *const u8,
    ) -> c_int;
    fn intercom_dtls_free(storage: *mut c_void);
    fn intercom_dtls_push_input(storage: *mut c_void, data: *const u8, length: usize) -> c_int;
    fn intercom_dtls_set_peer_transport_id(
        storage: *mut c_void,
        data: *const u8,
        length: usize,
    ) -> c_int;
    #[cfg(test)]
    fn intercom_dtls_set_mtu(storage: *mut c_void, mtu: u16);
    fn intercom_dtls_step(storage: *mut c_void, now_ms: u64) -> c_int;
    fn intercom_dtls_pop_output(storage: *mut c_void, output: *mut u8, capacity: usize) -> usize;
    fn intercom_dtls_exported_keys(storage: *mut c_void, output: *mut u8) -> c_int;
    fn intercom_dtls_export_diagnostic(
        storage: *mut c_void,
        called: *mut c_int,
        export_type: *mut c_int,
        prf: *mut c_int,
        error: *mut c_int,
    );
    fn intercom_dtls_debug_diagnostic(
        storage: *mut c_void,
        level: *mut c_int,
        line: *mut c_int,
        message: *mut u8,
        capacity: usize,
    ) -> usize;
    fn intercom_dtls_local_fingerprint(storage: *mut c_void, output: *mut u8) -> c_int;
    fn intercom_dtls_peer_fingerprint(storage: *mut c_void, output: *mut u8) -> c_int;
}

#[cfg(test)]
mod tests {
    use super::{DtlsRole, DtlsSrtpEngine, DtlsSrtpResources};
    use core::sync::atomic::Ordering;

    #[test]
    fn dtls_1_2_client_and_server_complete_handshake_and_export_matching_srtp_keys() {
        // Arrange
        let _guard = super::DTLS_TEST_LOCK
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let mut server_resources = DtlsSrtpResources::new();
        let mut client_resources = DtlsSrtpResources::new();
        let mut server = DtlsSrtpEngine::new(DtlsRole::Server, [0x42; 32], &mut server_resources)
            .expect("server init");
        let mut client = DtlsSrtpEngine::new(DtlsRole::Client, [0x99; 32], &mut client_resources)
            .expect("client init");
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
        assert_eq!(server.export_diagnostic(), (true, 0, 2, 0));
        assert_eq!(client.export_diagnostic(), (true, 0, 2, 0));
        let client_keys = client.exported_srtp_keys().expect("client keys");
        let server_keys = server.exported_srtp_keys().expect("server keys");

        // Assert
        assert_eq!(client_keys, server_keys);
        assert_eq!(client.peer_cert_sha256(), Some(server.local_cert_sha256()));
        assert_eq!(server.peer_cert_sha256(), Some(client.local_cert_sha256()));
    }

    #[test]
    fn server_reassembles_fragmented_dtls_client_hello() {
        // Arrange
        let _guard = super::DTLS_TEST_LOCK
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let mut server_resources = DtlsSrtpResources::new();
        let mut client_resources = DtlsSrtpResources::new();
        let mut server = DtlsSrtpEngine::new(DtlsRole::Server, [0x42; 32], &mut server_resources)
            .expect("server init");
        let mut client = DtlsSrtpEngine::new(DtlsRole::Client, [0x99; 32], &mut client_resources)
            .expect("client init");
        server
            .set_peer_transport_id(b"verified-ice-peer")
            .expect("server peer id");
        let mut client_hello = [0; 1500];
        client.step(0).expect("client step");
        let client_hello_length = client
            .pop_outbound_datagram(&mut client_hello)
            .expect("client hello datagram");
        assert_eq!(client_hello[0], 22, "DTLS handshake record");
        assert_eq!(client_hello[13], 1, "ClientHello message");
        let message_length = read_u24(&client_hello[14..17]);
        assert_eq!(client_hello_length, 25 + message_length);
        let split = message_length / 2;
        let mut first_fragment = [0; 1500];
        let mut second_fragment = [0; 1500];
        let first_length = make_client_hello_fragment(
            &client_hello[..client_hello_length],
            0,
            split,
            &mut first_fragment,
        );
        let second_length = make_client_hello_fragment(
            &client_hello[..client_hello_length],
            split,
            message_length - split,
            &mut second_fragment,
        );

        // Act
        server
            .push_inbound_datagram(&first_fragment[..first_length])
            .expect("first ClientHello fragment");
        let waiting_for_fragment = server.step(0);
        server
            .push_inbound_datagram(&second_fragment[..second_length])
            .expect("second ClientHello fragment");
        let completed_client_hello = server.step(1);
        let mut server_response = [0; 1500];
        let response_length = server.pop_outbound_datagram(&mut server_response);

        // Assert
        assert_eq!(waiting_for_fragment, Ok(false));
        assert_eq!(completed_client_hello, Ok(false)); // HelloVerifyRequest is pending.
        assert!(response_length.is_some());
    }

    #[test]
    fn server_reports_backpressure_without_overwriting_active_datagram() {
        // Arrange
        let _guard = super::DTLS_TEST_LOCK
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let mut server_resources = DtlsSrtpResources::new();
        let mut client_resources = DtlsSrtpResources::new();
        let mut server = DtlsSrtpEngine::new(DtlsRole::Server, [0x42; 32], &mut server_resources)
            .expect("server init");
        let mut client = DtlsSrtpEngine::new(DtlsRole::Client, [0x99; 32], &mut client_resources)
            .expect("client init");
        server
            .set_peer_transport_id(b"verified-ice-peer")
            .expect("server peer id");
        let mut client_hello = [0; 1500];
        client.step(0).expect("client step");
        let client_hello_length = client
            .pop_outbound_datagram(&mut client_hello)
            .expect("client hello datagram");

        // Act
        server
            .push_inbound_datagram(&client_hello[..client_hello_length])
            .expect("first ClientHello datagram");
        let queued = server.push_inbound_datagram(&client_hello[..client_hello_length]);

        // Assert
        assert_eq!(queued, Err(super::DtlsError::InputFailed(-0x6900)));
        assert_eq!(server.step(0), Ok(false));
        let mut server_response = [0; 1500];
        assert!(server.pop_outbound_datagram(&mut server_response).is_some());
    }

    fn read_u24(bytes: &[u8]) -> usize {
        (usize::from(bytes[0]) << 16) | (usize::from(bytes[1]) << 8) | usize::from(bytes[2])
    }

    fn write_u24(output: &mut [u8], value: usize) {
        output[0] = (value >> 16) as u8;
        output[1] = (value >> 8) as u8;
        output[2] = value as u8;
    }

    fn make_client_hello_fragment(
        client_hello: &[u8],
        offset: usize,
        fragment_length: usize,
        output: &mut [u8; 1500],
    ) -> usize {
        output[..13].copy_from_slice(&client_hello[..13]);
        output[13] = client_hello[13];
        output[14..17].copy_from_slice(&client_hello[14..17]);
        output[17..19].copy_from_slice(&client_hello[17..19]);
        write_u24(&mut output[19..22], offset);
        write_u24(&mut output[22..25], fragment_length);
        output[25..25 + fragment_length]
            .copy_from_slice(&client_hello[25 + offset..25 + offset + fragment_length]);
        output[11..13].copy_from_slice(&((12 + fragment_length) as u16).to_be_bytes());
        25 + fragment_length
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
    fn cached_identity_handshake_reuses_fingerprint_and_exports_matching_srtp_keys() {
        let _guard = super::DTLS_TEST_LOCK
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let mut gen_resources = DtlsSrtpResources::new();
        let identity =
            super::DtlsIdentity::generate([0x42; 32], &mut gen_resources).expect("generate identity");
        assert!(!identity.cert_der().is_empty());
        assert!(!identity.key_der().is_empty());
        assert_ne!(identity.fingerprint_sha256(), [0u8; 32]);

        let mut server_resources = DtlsSrtpResources::new();
        let mut client_resources = DtlsSrtpResources::new();
        let mut server =
            DtlsSrtpEngine::new_with_identity(DtlsRole::Server, [0x42; 32], &identity, &mut server_resources)
                .expect("server init with identity");
        let mut client =
            DtlsSrtpEngine::new_with_identity(DtlsRole::Client, [0x99; 32], &identity, &mut client_resources)
                .expect("client init with identity");
        assert_eq!(server.local_cert_sha256(), identity.fingerprint_sha256());
        assert_eq!(client.local_cert_sha256(), identity.fingerprint_sha256());

        server
            .set_peer_transport_id(b"verified-ice-peer")
            .expect("server peer id");
        let _ = client.set_peer_transport_id(b"client-peer-id");

        let mut wire_buffer = [0u8; 1500];
        let mut client_done = false;
        let mut server_done = false;
        for tick in 0..64u64 {
            if !client_done {
                client_done = client.step(tick * 10).expect("client step");
            }
            while let Some(len) = client.pop_outbound_datagram(&mut wire_buffer) {
                server
                    .push_inbound_datagram(&wire_buffer[..len])
                    .expect("server push");
            }
            if !server_done {
                server_done = server.step(tick * 10).expect("server step");
            }
            while let Some(len) = server.pop_outbound_datagram(&mut wire_buffer) {
                client
                    .push_inbound_datagram(&wire_buffer[..len])
                    .expect("client push");
            }
            if client_done && server_done {
                break;
            }
        }
        assert!(client_done && server_done);
        let server_keys = server.exported_srtp_keys().expect("server keys");
        let client_keys = client.exported_srtp_keys().expect("client keys");
        assert_eq!(server_keys, client_keys);
    }

    #[test]
    fn multi_datagram_server_flight_queues_without_want_write() {
        let _guard = super::DTLS_TEST_LOCK
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let mut server_resources = DtlsSrtpResources::new();
        let mut client_resources = DtlsSrtpResources::new();
        let mut server = DtlsSrtpEngine::new(DtlsRole::Server, [0x42; 32], &mut server_resources)
            .expect("server init");
        let mut client = DtlsSrtpEngine::new(DtlsRole::Client, [0x99; 32], &mut client_resources)
            .expect("client init");
        server
            .set_peer_transport_id(b"verified-ice-peer")
            .expect("server peer id");

        let mut wire = [0u8; 1500];
        // 1. ClientHello (initial) -> Server HelloVerifyRequest
        client.step(0).expect("client step 0");
        let len = client.pop_outbound_datagram(&mut wire).expect("ch1");
        server.push_inbound_datagram(&wire[..len]).expect("srv ch1");
        server.step(0).expect("srv hvr");
        let len = server.pop_outbound_datagram(&mut wire).expect("hvr");
        client.push_inbound_datagram(&wire[..len]).expect("cli hvr");

        // 2. ClientHello (with cookie) -> Server multi-datagram flight at 300-byte MTU
        server.set_mtu(300);
        client.step(1).expect("client step 1");
        let len = client.pop_outbound_datagram(&mut wire).expect("ch2");
        server.push_inbound_datagram(&wire[..len]).expect("srv ch2");

        server.step(1).expect("server flight step");
        let mut popped = 0usize;
        while server.pop_outbound_datagram(&mut wire).is_some() {
            popped += 1;
        }
        assert!(
            popped >= 2,
            "expected multi-datagram flight in one step, got {popped}"
        );
    }

    #[test]
    fn allocator_arena_is_released_and_can_be_reinitialized() {
        // Arrange
        let _guard = super::DTLS_TEST_LOCK
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let (heap_ptr, _) = super::heap_buffer();

        // Act
        let failed_init = super::initialize_mbedtls_heap(heap_ptr, 128);
        let reset_after_failure = super::HEAP_STATE.load(Ordering::Acquire);
        let mut first_resources = DtlsSrtpResources::new();
        let first = DtlsSrtpEngine::new(DtlsRole::Client, [0x21; 32], &mut first_resources);
        let first = first.expect("engine after failed arena initialization");
        drop(first);
        let mut second_resources = DtlsSrtpResources::new();
        let mut second = DtlsSrtpEngine::new(DtlsRole::Client, [0x22; 32], &mut second_resources)
            .expect("second engine");

        // Assert
        assert_eq!(failed_init, Err(super::DtlsError::HeapInitializationFailed));
        assert_eq!(reset_after_failure, 0);
        assert!(!second.step(0).expect("step after arena reinitialization"));
    }

    #[test]
    fn benchmark_cached_vs_uncached_startup() {
        let _guard = super::DTLS_TEST_LOCK
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let mut gen_resources = DtlsSrtpResources::new();
        let identity =
            super::DtlsIdentity::generate([0x42; 32], &mut gen_resources).expect("identity");

        let iterations = 10u32;
        let mut uncached_total_us = 0u128;
        let mut cached_total_us = 0u128;
        for i in 0..iterations {
            let mut r1 = DtlsSrtpResources::new();
            let t0 = std::time::Instant::now();
            let engine = DtlsSrtpEngine::new(DtlsRole::Server, [i as u8; 32], &mut r1)
                .expect("uncached init");
            uncached_total_us += t0.elapsed().as_micros();
            drop(engine);

            let mut r2 = DtlsSrtpResources::new();
            let t1 = std::time::Instant::now();
            let engine = DtlsSrtpEngine::new_with_identity(
                DtlsRole::Server,
                [i as u8; 32],
                &identity,
                &mut r2,
            )
            .expect("cached init");
            cached_total_us += t1.elapsed().as_micros();
            drop(engine);
        }
        let uncached_avg_us = uncached_total_us / u128::from(iterations);
        let cached_avg_us = cached_total_us / u128::from(iterations);

        // Multi-datagram handshake step count: popping 1 datagram per step vs draining all per step.
        let count_handshake_steps = |drain_all: bool| -> usize {
            let mut srv_res = DtlsSrtpResources::new();
            let mut cli_res = DtlsSrtpResources::new();
            let mut srv = DtlsSrtpEngine::new_with_identity(
                DtlsRole::Server,
                [0x42; 32],
                &identity,
                &mut srv_res,
            )
            .expect("srv");
            let mut cli = DtlsSrtpEngine::new_with_identity(
                DtlsRole::Client,
                [0x99; 32],
                &identity,
                &mut cli_res,
            )
            .expect("cli");
            srv.set_mtu(300);
            cli.set_mtu(300);
            srv.set_peer_transport_id(b"verified-ice-peer")
                .expect("peer id");
            let mut wire = [0u8; 1500];
            let mut steps = 0usize;
            let mut cli_done = false;
            let mut srv_done = false;
            while !(cli_done && srv_done) && steps < 64 {
                steps += 1;
                if !cli_done {
                    cli_done = cli.step(steps as u64 * 10).expect("cli step");
                }
                if drain_all {
                    while let Some(len) = cli.pop_outbound_datagram(&mut wire) {
                        srv.push_inbound_datagram(&wire[..len]).expect("srv push");
                        if !srv_done {
                            srv_done = srv.step(steps as u64 * 10).expect("srv step");
                        }
                    }
                } else if let Some(len) = cli.pop_outbound_datagram(&mut wire) {
                    srv.push_inbound_datagram(&wire[..len]).expect("srv push");
                    if !srv_done {
                        srv_done = srv.step(steps as u64 * 10).expect("srv step");
                    }
                }
                if !srv_done {
                    srv_done = srv.step(steps as u64 * 10).expect("srv step");
                }
                if drain_all {
                    while let Some(len) = srv.pop_outbound_datagram(&mut wire) {
                        cli.push_inbound_datagram(&wire[..len]).expect("cli push");
                        if !cli_done {
                            cli_done = cli.step(steps as u64 * 10).expect("cli step");
                        }
                    }
                } else if let Some(len) = srv.pop_outbound_datagram(&mut wire) {
                    cli.push_inbound_datagram(&wire[..len]).expect("cli push");
                    if !cli_done {
                        cli_done = cli.step(steps as u64 * 10).expect("cli step");
                    }
                }
            }
            assert!(cli_done && srv_done);
            steps
        };

        let single_pop_steps = count_handshake_steps(false);
        let multi_drain_steps = count_handshake_steps(true);
        std::println!(
            "DTLS startup benchmark: uncached_avg_us={uncached_avg_us} cached_avg_us={cached_avg_us} single_pop_steps={single_pop_steps} multi_drain_steps={multi_drain_steps}"
        );
        assert!(
            cached_avg_us < uncached_avg_us,
            "expected cached init ({cached_avg_us} us) < uncached ({uncached_avg_us} us)"
        );
        assert!(
            multi_drain_steps < single_pop_steps,
            "expected multi-drain steps ({multi_drain_steps}) < single-pop steps ({single_pop_steps})"
        );
    }
}
