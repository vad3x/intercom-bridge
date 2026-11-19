//! Persistent failure diagnostics: hardware WDT, RTC Fast RAM breadcrumb,
//! flash persistence across power loss, and silent network-stall detection.

use core::fmt::Write;

/// Format a compact diagnostic failure summary into `out`, returning bytes written.
pub fn format_failure_summary(
    out: &mut [u8],
    tag: &str,
    uptime_sec: u64,
    free_heap: usize,
    link_up: bool,
    ipv4: Option<[u8; 4]>,
    log_tail: &str,
) -> usize {
    struct BufWriter<'a> {
        buf: &'a mut [u8],
        pos: usize,
    }

    impl Write for BufWriter<'_> {
        fn write_str(&mut self, s: &str) -> core::fmt::Result {
            let bytes = s.as_bytes();
            let avail = self.buf.len().saturating_sub(self.pos);
            let copy_len = bytes.len().min(avail);
            self.buf[self.pos..self.pos + copy_len].copy_from_slice(&bytes[..copy_len]);
            self.pos += copy_len;
            Ok(())
        }
    }

    let mut w = BufWriter { buf: out, pos: 0 };
    if let Some(ip) = ipv4 {
        let _ = write!(
            w,
            "{tag} up={uptime_sec}s heap={free_heap} link={link_up} ip={}.{}.{}.{}",
            ip[0], ip[1], ip[2], ip[3]
        );
    } else {
        let _ = write!(
            w,
            "{tag} up={uptime_sec}s heap={free_heap} link={link_up} ip=none"
        );
    }

    let trimmed = log_tail.trim();
    if !trimmed.is_empty() {
        let _ = write!(w, " | tail: ");
        let avail = w.buf.len().saturating_sub(w.pos);
        if avail > 0 {
            let bytes = trimmed.as_bytes();
            let start = bytes.len().saturating_sub(avail);
            let mut safe_start = start;
            while safe_start < bytes.len() && !trimmed.is_char_boundary(safe_start) {
                safe_start += 1;
            }
            let _ = w.write_str(&trimmed[safe_start..]);
        }
    }

    w.pos
}

/// Build a minimal 27-byte DNS A-record query for `localhost.` on UDP port 53.
pub fn build_dns_probe_query(tx_id: u16) -> [u8; 27] {
    let [id_hi, id_lo] = tx_id.to_be_bytes();
    [
        id_hi, id_lo, 0x01, 0x00, // ID, standard query, RD=1
        0x00, 0x01, 0x00, 0x00, // QDCOUNT=1, ANCOUNT=0
        0x00, 0x00, 0x00, 0x00, // NSCOUNT=0, ARCOUNT=0
        0x09, b'l', b'o', b'c', b'a', b'l', b'h', b'o', b's', b't', 0x00, // "localhost."
        0x00, 0x01, // QTYPE=A
        0x00, 0x01, // QCLASS=IN
    ]
}

#[cfg(target_arch = "riscv32")]
mod target {
    use core::ptr::{addr_of, addr_of_mut};

    use rs_matter_embassy::enet::net::udp::{PacketMetadata, UdpSocket};
    use rs_matter_embassy::enet::net::{IpAddress, IpEndpoint};
    use embassy_time::{Duration, Instant, Timer};
    use esp_hal::rtc_cntl::SocResetReason;
    use esp_hal::timer::timg::{MwdtStage, Wdt};
    use log::{info, warn};

    use super::{build_dns_probe_query, format_failure_summary};

    const RTC_MAGIC: u32 = 0x4943_4D44; // "ICMD"
    const WDT_TIMEOUT_SECS: u64 = 30;
    const FEED_INTERVAL_SECS: u64 = 5;
    const PROBE_EVERY_TICKS: u32 = 6; // 30s
    const STALL_THRESHOLD_PROBES: u32 = 4; // 2m of zero DNS/gateway UDP replies while link_up=true
    const RELOG_EVERY_TICKS: u32 = 360; // 30m

    #[repr(C)]
    #[derive(Clone, Copy)]
    struct RtcBreadcrumb {
        magic: u32,
        uptime_sec: u64,
        free_heap: u32,
        link_up: bool,
        has_ipv4: bool,
        ipv4: [u8; 4],
        phase: u8, // 0=connecting, 1=operational, 2=net_stall, 3=panic
    }

    impl RtcBreadcrumb {
        const EMPTY: Self = Self {
            magic: 0,
            uptime_sec: 0,
            free_heap: 0,
            link_up: false,
            has_ipv4: false,
            ipv4: [0; 4],
            phase: 0,
        };
    }

    #[esp_hal::ram(unstable(rtc_fast))]
    static mut RTC_BREADCRUMB: RtcBreadcrumb = RtcBreadcrumb::EMPTY;

    fn read_rtc_breadcrumb() -> Option<RtcBreadcrumb> {
        // SAFETY: Single-core ESP32-C6; accessed only from main task / panic halt.
        let bc = unsafe { core::ptr::read_volatile(addr_of!(RTC_BREADCRUMB)) };
        (bc.magic == RTC_MAGIC).then_some(bc)
    }

    fn write_rtc_breadcrumb(bc: RtcBreadcrumb) {
        // SAFETY: Single-core ESP32-C6; accessed only from main task / panic halt.
        unsafe { core::ptr::write_volatile(addr_of_mut!(RTC_BREADCRUMB), bc) };
    }

    fn is_wdt_reason(reason: SocResetReason) -> bool {
        matches!(
            reason,
            SocResetReason::CoreMwdt0
                | SocResetReason::CoreMwdt1
                | SocResetReason::CoreRtcWdt
                | SocResetReason::Cpu0Mwdt0
                | SocResetReason::Cpu0RtcWdt
        )
    }

    /// Inspect reset reason and RTC Fast RAM on boot, persist any WDT freeze to
    /// `system_kvs` flash, and log any persisted failure summary into `uptime_logger`.
    pub fn record_boot_diagnostics() -> bool {
        let reason = esp_hal::system::reset_reason();
        let prev = read_rtc_breadcrumb();

        if let Some(r) = reason {
            info!("Reset reason: {:?}", r);
            if is_wdt_reason(r) {
                let mut buf = [0u8; 512];
                let (up, heap, link, ip) = if let Some(bc) = prev {
                    (
                        bc.uptime_sec,
                        bc.free_heap as usize,
                        bc.link_up,
                        bc.has_ipv4.then_some(bc.ipv4),
                    )
                } else {
                    (0, 0, false, None)
                };
                let len = format_failure_summary(
                    &mut buf,
                    "wdt_reset",
                    up,
                    heap,
                    link,
                    ip,
                    "",
                );
                if let Ok(summary) = core::str::from_utf8(&buf[..len]) {
                    crate::reset::persist_last_failure(summary);
                }
            }
        }

        write_rtc_breadcrumb(RtcBreadcrumb {
            magic: RTC_MAGIC,
            uptime_sec: 0,
            free_heap: esp_alloc::HEAP.free() as u32,
            link_up: false,
            has_ipv4: false,
            ipv4: [0; 4],
            phase: 0,
        });

        let mut read_buf = [0u8; 1024];
        if let Some(summary) = crate::reset::read_persisted_last_failure(&mut read_buf) {
            warn!("Persisted last failure: {}", summary);
            true
        } else {
            false
        }
    }

    /// Persist panic context + log tail to `system_kvs` flash before resetting.
    pub fn persist_panic_and_reset() -> ! {
        let uptime_sec = Instant::now().as_secs();
        let free_heap = esp_alloc::HEAP.free();
        let prev = read_rtc_breadcrumb();
        let (link_up, ipv4) = prev
            .map(|bc| (bc.link_up, bc.has_ipv4.then_some(bc.ipv4)))
            .unwrap_or((false, None));

        let mut log_buf = [0u8; 512];
        let log_len = crate::utils::uptime_logger::get_buffered_logs(&mut log_buf);
        let log_tail = core::str::from_utf8(&log_buf[..log_len]).unwrap_or("");

        let mut summary_buf = [0u8; 512];
        let len = format_failure_summary(
            &mut summary_buf,
            "panic",
            uptime_sec,
            free_heap,
            link_up,
            ipv4,
            log_tail,
        );
        if let Ok(summary) = core::str::from_utf8(&summary_buf[..len]) {
            crate::reset::persist_last_failure(summary);
        }

        esp_hal::system::software_reset();
    }

    /// Feed the hardware WDT, keep RTC Fast RAM updated, and detect silent network stalls.
    pub async fn run_diag_monitor(
        mut wdt: Wdt<esp_hal::peripherals::TIMG0<'static>>,
        has_last_failure: bool,
    ) {
        wdt.set_timeout(
            MwdtStage::Stage0,
            esp_hal::time::Duration::from_secs(WDT_TIMEOUT_SECS),
        );
        wdt.enable();
        wdt.feed();

        let stack = loop {
            if let Some(stack) = rs_matter_embassy::enet::global_stack() {
                break stack;
            }
            wdt.feed();
            Timer::after(Duration::from_secs(2)).await;
        };

        let mut tick: u32 = 0;
        let mut stall_count: u32 = 0;
        let mut tx_id: u16 = 1;

        loop {
            wdt.feed();
            Timer::after(Duration::from_secs(FEED_INTERVAL_SECS)).await;
            wdt.feed();

            tick = tick.wrapping_add(1);
            let uptime_sec = Instant::now().as_secs();
            let free_heap = esp_alloc::HEAP.free();
            let link_up = stack.is_link_up();
            let v4 = stack.config_v4();
            let ipv4 = v4.as_ref().map(|c| c.address.address().octets());

            write_rtc_breadcrumb(RtcBreadcrumb {
                magic: RTC_MAGIC,
                uptime_sec,
                free_heap: free_heap as u32,
                link_up,
                has_ipv4: ipv4.is_some(),
                ipv4: ipv4.unwrap_or([0; 4]),
                phase: if link_up { 1 } else { 0 },
            });

            if tick % PROBE_EVERY_TICKS == 0 && link_up {
                if let Some(cfg) = v4.as_ref() {
                    let probe_target = cfg
                        .dns_servers
                        .first()
                        .copied()
                        .or(cfg.gateway);
                    if let Some(target_v4) = probe_target {
                        let ok = probe_udp_dns(stack, target_v4, tx_id).await;
                        wdt.feed();
                        tx_id = tx_id.wrapping_add(1);
                        if ok {
                            stall_count = 0;
                        } else {
                            stall_count = stall_count.saturating_add(1);
                            warn!(
                                "Network liveness probe timed out ({}/{}): link_up=true target={}",
                                stall_count, STALL_THRESHOLD_PROBES, target_v4
                            );
                            if stall_count >= STALL_THRESHOLD_PROBES {
                                let mut log_buf = [0u8; 512];
                                let log_len =
                                    crate::utils::uptime_logger::get_buffered_logs(&mut log_buf);
                                let log_tail =
                                    core::str::from_utf8(&log_buf[..log_len]).unwrap_or("");
                                let mut summary_buf = [0u8; 512];
                                let len = format_failure_summary(
                                    &mut summary_buf,
                                    "net_stall",
                                    uptime_sec,
                                    free_heap,
                                    link_up,
                                    ipv4,
                                    log_tail,
                                );
                                if let Ok(summary) = core::str::from_utf8(&summary_buf[..len]) {
                                    crate::reset::persist_last_failure(summary);
                                }
                                esp_hal::system::software_reset();
                            }
                        }
                    }
                }
            } else if !link_up {
                stall_count = 0;
            }

            if has_last_failure && tick % RELOG_EVERY_TICKS == 0 {
                let mut read_buf = [0u8; 1024];
                if let Some(summary) = crate::reset::read_persisted_last_failure(&mut read_buf) {
                    info!("Persisted last failure: {}", summary);
                }
            }
        }
    }

    async fn probe_udp_dns(
        stack: rs_matter_embassy::enet::net::Stack<'static>,
        target: core::net::Ipv4Addr,
        tx_id: u16,
    ) -> bool {
        let mut rx_meta = [PacketMetadata::EMPTY; 1];
        let mut tx_meta = [PacketMetadata::EMPTY; 1];
        let mut rx_buf = [0u8; 128];
        let mut tx_buf = [0u8; 64];

        let mut sock = UdpSocket::new(stack, &mut rx_meta, &mut rx_buf, &mut tx_meta, &mut tx_buf);
        if sock.bind(0).is_err() {
            return true; // Do not treat ephemeral bind failure as a network stall
        }

        let query = build_dns_probe_query(tx_id);
        let remote = IpEndpoint::new(IpAddress::Ipv4(target), 53);
        if sock.send_to(&query, remote).await.is_err() {
            return false;
        }

        let mut resp = [0u8; 128];
        matches!(
            embassy_time::with_timeout(Duration::from_secs(3), sock.recv_from(&mut resp)).await,
            Ok(Ok(_))
        )
    }
}

#[cfg(target_arch = "riscv32")]
pub use target::{persist_panic_and_reset, record_boot_diagnostics, run_diag_monitor};

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn format_failure_summary_truncates_and_preserves_header() {
        let mut out = [0u8; 128];
        let len = format_failure_summary(
            &mut out,
            "net_stall",
            1234,
            144000,
            true,
            Some([192, 168, 0, 150]),
            "line1\nline2\nlast-log-line",
        );
        let s = core::str::from_utf8(&out[..len]).unwrap();
        assert!(s.starts_with("net_stall up=1234s heap=144000 link=true ip=192.168.0.150"));
        assert!(s.contains("last-log-line"));
    }

    #[test]
    fn build_dns_probe_query_encodes_tx_id_and_localhost() {
        let q = build_dns_probe_query(0x1234);
        assert_eq!(q[0], 0x12);
        assert_eq!(q[1], 0x34);
        assert_eq!(&q[13..22], b"localhost");
    }
}
