// Copyright 2018 Amazon.com, Inc. or its affiliates. All Rights Reserved.
// SPDX-License-Identifier: Apache-2.0

//! Contains logic that helps with handling ARP frames over Ethernet, which encapsulate requests
//! or replies related to IPv4 addresses.
//!
//! A more detailed view of an ARP frame can be found [here].
//!
//! [here]: https://en.wikipedia.org/wiki/Address_Resolution_Protocol
use std::convert::From;
use std::fmt::Debug;
use std::net::Ipv4Addr;
use std::result::Result;

use super::bytes::{InnerBytes, NetworkBytes, NetworkBytesMut};
use super::ethernet::{self, ETHERTYPE_IPV4};
use crate::utils::net::mac::{MAC_ADDR_LEN, MacAddr};

/// ARP Request operation
pub const OPER_REQUEST: u16 = 0x0001;

/// ARP Reply operation
pub const OPER_REPLY: u16 = 0x0002;

/// ARP is for Ethernet hardware
pub const HTYPE_ETHERNET: u16 = 0x0001;

/// The length of an ARP frame for IPv4 over Ethernet.
pub const ETH_IPV4_FRAME_LEN: usize = 28;

const HTYPE_OFFSET: usize = 0;
const PTYPE_OFFSET: usize = 2;
const HLEN_OFFSET: usize = 4;
const PLEN_OFFSET: usize = 5;
const OPER_OFFSET: usize = 6;
const SHA_OFFSET: usize = 8;

// The following constants are specific to ARP requests/responses
// associated with IPv4 over Ethernet.
const ETH_IPV4_SPA_OFFSET: usize = 14;
const ETH_IPV4_THA_OFFSET: usize = 18;
const ETH_IPV4_TPA_OFFSET: usize = 24;

const IPV4_ADDR_LEN: u8 = 4;

/// Represents errors which may occur while parsing or writing a frame.
#[derive(Debug, PartialEq, Eq, thiserror::Error, displaydoc::Display)]
pub enum ArpError {
    /// Invalid hardware address length.
    HLen,
    /// Invalid hardware type.
    HType,
    /// Invalid operation.
    Operation,
    /// Invalid protocol address length.
    PLen,
    /// Invalid protocol type.
    PType,
    /// The provided slice does not fit the size of a frame.
    SliceExactLen,
}

/// The inner bytes will be interpreted as an ARP frame.
///
/// ARP is a generic protocol as far as data
/// link layer and network layer protocols go, but this particular implementation is concerned with
/// ARP frames related to IPv4 over Ethernet.
#[derive(Debug)]
pub struct EthIPv4ArpFrame<'a, T: 'a> {
    bytes: InnerBytes<'a, T>,
}

#[allow(clippy::len_without_is_empty)]
impl<T: NetworkBytes + Debug> EthIPv4ArpFrame<'_, T> {
    /// Interprets the given bytes as an ARP frame, without doing any validity checks beforehand.
    ///
    ///  # Panics
    ///
    /// This method does not panic, but further method calls on the resulting object may panic if
    /// `bytes` contains invalid input.
    #[inline]
    pub fn from_bytes_unchecked(bytes: T) -> Self {
        EthIPv4ArpFrame {
            bytes: InnerBytes::new(bytes),
        }
    }

    /// Tries to interpret a byte slice as a valid IPv4 over Ethernet ARP request.
    ///
    /// If no error occurs, it guarantees accessor methods (which make use of various `_unchecked`
    /// functions) are safe to call on the result, because all predefined offsets will be valid.
    pub fn request_from_bytes(bytes: T) -> Result<Self, ArpError> {
        // This kind of frame has a fixed length, so we know what to expect.
        if bytes.len() != ETH_IPV4_FRAME_LEN {
            return Err(ArpError::SliceExactLen);
        }

        let maybe = EthIPv4ArpFrame::from_bytes_unchecked(bytes);

        if maybe.htype() != HTYPE_ETHERNET {
            return Err(ArpError::HType);
        }

        if maybe.ptype() != ETHERTYPE_IPV4 {
            return Err(ArpError::PType);
        }

        // We could theoretically skip the hlen and plen checks, since they are kinda implicit.
        if maybe.hlen() != MAC_ADDR_LEN {
            return Err(ArpError::HLen);
        }

        if maybe.plen() != IPV4_ADDR_LEN {
            return Err(ArpError::PLen);
        }

        if maybe.operation() != OPER_REQUEST {
            return Err(ArpError::Operation);
        }

        Ok(maybe)
    }

    /// Returns the hardware type of the frame.
    #[inline]
    pub fn htype(&self) -> u16 {
        self.bytes.ntohs_unchecked(HTYPE_OFFSET)
    }

    /// Returns the protocol type of the frame.
    #[inline]
    pub fn ptype(&self) -> u16 {
        self.bytes.ntohs_unchecked(PTYPE_OFFSET)
    }

    /// Returns the hardware address length of the frame.
    #[inline]
    pub fn hlen(&self) -> u8 {
        self.bytes[HLEN_OFFSET]
    }

    /// Returns the protocol address length of the frame.
    #[inline]
    pub fn plen(&self) -> u8 {
        self.bytes[PLEN_OFFSET]
    }

    /// Returns the type of operation within the frame.
    #[inline]
    pub fn operation(&self) -> u16 {
        self.bytes.ntohs_unchecked(OPER_OFFSET)
    }

    /// Returns the sender hardware address.
    #[inline]
    pub fn sha(&self) -> MacAddr {
        MacAddr::from_bytes_unchecked(&self.bytes[SHA_OFFSET..ETH_IPV4_SPA_OFFSET])
    }

    /// Returns the sender protocol address.
    #[inline]
    pub fn spa(&self) -> Ipv4Addr {
        Ipv4Addr::from(self.bytes.ntohl_unchecked(ETH_IPV4_SPA_OFFSET))
    }

    /// Returns the target hardware address.
    #[inline]
    pub fn tha(&self) -> MacAddr {
        MacAddr::from_bytes_unchecked(&self.bytes[ETH_IPV4_THA_OFFSET..ETH_IPV4_TPA_OFFSET])
    }

    /// Returns the target protocol address.
    #[inline]
    pub fn tpa(&self) -> Ipv4Addr {
        Ipv4Addr::from(self.bytes.ntohl_unchecked(ETH_IPV4_TPA_OFFSET))
    }

    /// Returns the length of the frame.
    #[inline]
    pub fn len(&self) -> usize {
        // This might as well return ETH_IPV4_FRAME_LEN directly, since we check this is the actual
        // length in request_from_bytes(). For some reason it seems nicer leaving it as is.
        self.bytes.len()
    }
}

impl<T: NetworkBytesMut + Debug> EthIPv4ArpFrame<'_, T> {
    #[allow(clippy::too_many_arguments)]
    fn write_raw(
        buf: T,
        htype: u16,
        ptype: u16,
        hlen: u8,
        plen: u8,
        operation: u16,
        sha: MacAddr,
        spa: Ipv4Addr,
        tha: MacAddr,
        tpa: Ipv4Addr,
    ) -> Result<Self, ArpError> {
        if buf.len() != ETH_IPV4_FRAME_LEN {
            return Err(ArpError::SliceExactLen);
        }

        // This is ok, because we've checked the length of the slice.
        let mut frame = EthIPv4ArpFrame::from_bytes_unchecked(buf);

        frame.set_htype(htype);
        frame.set_ptype(ptype);
        frame.set_hlen(hlen);
        frame.set_plen(plen);
        frame.set_operation(operation);
        frame.set_sha(sha);
        frame.set_spa(spa);
        frame.set_tha(tha);
        frame.set_tpa(tpa);

        Ok(frame)
    }

    /// Attempts to write an ARP request to `buf`, based on the specified hardware and protocol
    /// addresses.
    #[inline]
    pub fn write_request(
        buf: T,
        sha: MacAddr,
        spa: Ipv4Addr,
        tha: MacAddr,
        tpa: Ipv4Addr,
    ) -> Result<Self, ArpError> {
        Self::write_raw(
            buf,
            HTYPE_ETHERNET,
            ETHERTYPE_IPV4,
            MAC_ADDR_LEN,
            IPV4_ADDR_LEN,
            OPER_REQUEST,
            sha,
            spa,
            tha,
            tpa,
        )
    }

    /// Attempts to write an ARP reply to `buf`, based on the specified hardware and protocol
    /// addresses.
    #[inline]
    pub fn write_reply(
        buf: T,
        sha: MacAddr,
        spa: Ipv4Addr,
        tha: MacAddr,
        tpa: Ipv4Addr,
    ) -> Result<Self, ArpError> {
        Self::write_raw(
            buf,
            HTYPE_ETHERNET,
            ETHERTYPE_IPV4,
            MAC_ADDR_LEN,
            IPV4_ADDR_LEN,
            OPER_REPLY,
            sha,
            spa,
            tha,
            tpa,
        )
    }

    /// Sets the hardware type of the frame.
    #[inline]
    pub fn set_htype(&mut self, value: u16) {
        self.bytes.htons_unchecked(HTYPE_OFFSET, value);
    }

    /// Sets the protocol type of the frame.
    #[inline]
    pub fn set_ptype(&mut self, value: u16) {
        self.bytes.htons_unchecked(PTYPE_OFFSET, value);
    }

    /// Sets the hardware address length of the frame.
    #[inline]
    pub fn set_hlen(&mut self, value: u8) {
        self.bytes[HLEN_OFFSET] = value;
    }

    /// Sets the protocol address length of the frame.
    #[inline]
    pub fn set_plen(&mut self, value: u8) {
        self.bytes[PLEN_OFFSET] = value;
    }

    /// Sets the operation within the frame.
    #[inline]
    pub fn set_operation(&mut self, value: u16) {
        self.bytes.htons_unchecked(OPER_OFFSET, value);
    }

    /// Sets the sender hardware address.
    #[inline]
    pub fn set_sha(&mut self, addr: MacAddr) {
        self.bytes[SHA_OFFSET..ETH_IPV4_SPA_OFFSET].copy_from_slice(addr.get_bytes());
    }

    /// Sets the sender protocol address.
    #[inline]
    pub fn set_spa(&mut self, addr: Ipv4Addr) {
        self.bytes
            .htonl_unchecked(ETH_IPV4_SPA_OFFSET, u32::from(addr));
    }

    /// Sets the target hardware address.
    #[inline]
    pub fn set_tha(&mut self, addr: MacAddr) {
        self.bytes[ETH_IPV4_THA_OFFSET..ETH_IPV4_TPA_OFFSET].copy_from_slice(addr.get_bytes());
    }

    /// Sets the target protocol address.
    #[inline]
    pub fn set_tpa(&mut self, addr: Ipv4Addr) {
        self.bytes
            .htonl_unchecked(ETH_IPV4_TPA_OFFSET, u32::from(addr));
    }
}

/// This function checks if `buf` may hold an Ethernet frame which encapsulates an
/// `EthIPv4ArpRequest` for the given address. Cannot produce false negatives.
#[inline]
pub fn test_speculative_tpa(buf: &[u8], addr: Ipv4Addr) -> bool {
    // The unchecked methods are safe because we actually check the buffer length beforehand.
    if buf.len() >= ethernet::PAYLOAD_OFFSET + ETH_IPV4_FRAME_LEN {
        let bytes = &buf[ethernet::PAYLOAD_OFFSET..];
        if EthIPv4ArpFrame::from_bytes_unchecked(bytes).tpa() == addr {
            return true;
        }
    }
    false
}

#[cfg(test)]
mod tests {
    use std::str::FromStr;

    use super::*;

    #[test]
    fn test_eth_ipv4_arp_frame() {
        let mut a = [0u8; 1000];
        let mut bad_array = [0u8; 1];

        let sha = MacAddr::from_str("01:23:45:67:89:ab").unwrap();
        let tha = MacAddr::from_str("cd:ef:01:23:45:67").unwrap();
        let spa = Ipv4Addr::new(10, 1, 2, 3);
        let tpa = Ipv4Addr::new(10, 4, 5, 6);

        // Slice is too short.
        assert_eq!(
            EthIPv4ArpFrame::request_from_bytes(bad_array.as_ref()).unwrap_err(),
            ArpError::SliceExactLen
        );

        // Slice is too short.
        assert_eq!(
            EthIPv4ArpFrame::write_reply(bad_array.as_mut(), sha, spa, tha, tpa).unwrap_err(),
            ArpError::SliceExactLen
        );

        // Slice is too long.
        assert_eq!(
            EthIPv4ArpFrame::write_reply(a.as_mut(), sha, spa, tha, tpa).unwrap_err(),
            ArpError::SliceExactLen
        );

        // We write a valid ARP reply to the specified slice.
        {
            let f = EthIPv4ArpFrame::write_reply(&mut a[..ETH_IPV4_FRAME_LEN], sha, spa, tha, tpa)
                .unwrap();

            // This is a bit redundant given the following tests, but assert away!
            assert_eq!(f.htype(), HTYPE_ETHERNET);
            assert_eq!(f.ptype(), ETHERTYPE_IPV4);
            assert_eq!(f.hlen(), MAC_ADDR_LEN);
            assert_eq!(f.plen(), IPV4_ADDR_LEN);
            assert_eq!(f.operation(), OPER_REPLY);
            assert_eq!(f.sha(), sha);
            assert_eq!(f.spa(), spa);
            assert_eq!(f.tha(), tha);
            assert_eq!(f.tpa(), tpa);
        }

        // Now let's try to parse a request.

        // Slice is too long.
        assert_eq!(
            EthIPv4ArpFrame::request_from_bytes(a.as_ref()).unwrap_err(),
            ArpError::SliceExactLen
        );

        // The length is fine now, but the operation is a reply instead of request.
        assert_eq!(
            EthIPv4ArpFrame::request_from_bytes(&a[..ETH_IPV4_FRAME_LEN]).unwrap_err(),
            ArpError::Operation
        );

        // Various requests
        let requests = [
            (
                HTYPE_ETHERNET,
                ETHERTYPE_IPV4,
                MAC_ADDR_LEN,
                IPV4_ADDR_LEN,
                None,
            ), // Valid request
            (
                HTYPE_ETHERNET + 1,
                ETHERTYPE_IPV4,
                MAC_ADDR_LEN,
                IPV4_ADDR_LEN,
                Some(ArpError::HType),
            ), // Invalid htype
            (
                HTYPE_ETHERNET,
                ETHERTYPE_IPV4 + 1,
                MAC_ADDR_LEN,
                IPV4_ADDR_LEN,
                Some(ArpError::PType),
            ), // Invalid ptype
            (
                HTYPE_ETHERNET,
                ETHERTYPE_IPV4,
                MAC_ADDR_LEN + 1,
                IPV4_ADDR_LEN,
                Some(ArpError::HLen),
            ), // Invalid hlen
            (
                HTYPE_ETHERNET,
                ETHERTYPE_IPV4,
                MAC_ADDR_LEN,
                IPV4_ADDR_LEN + 1,
                Some(ArpError::PLen),
            ), // Invalid plen
        ];

        for (htype, ptype, hlen, plen, err) in requests.iter() {
            EthIPv4ArpFrame::write_raw(
                &mut a[..ETH_IPV4_FRAME_LEN],
                *htype,
                *ptype,
                *hlen,
                *plen,
                OPER_REQUEST,
                sha,
                spa,
                tha,
                tpa,
            )
            .unwrap();
            match err {
                None => {
                    EthIPv4ArpFrame::request_from_bytes(&a[..ETH_IPV4_FRAME_LEN]).unwrap();
                }
                Some(arp_error) => assert_eq!(
                    EthIPv4ArpFrame::request_from_bytes(&a[..ETH_IPV4_FRAME_LEN]).unwrap_err(),
                    *arp_error
                ),
            }
        }
    }

    #[test]
    fn test_speculative() {
        let mut a = [0u8; 1000];
        let addr = Ipv4Addr::new(1, 2, 3, 4);

        assert!(!test_speculative_tpa(a.as_ref(), addr));

        {
            let mac = MacAddr::from_bytes_unchecked(&[0; 6]);
            let mut eth = crate::dumbo::pdu::ethernet::EthernetFrame::write_incomplete(
                a.as_mut(),
                mac,
                mac,
                0,
            )
            .unwrap();
            let mut arp = EthIPv4ArpFrame::from_bytes_unchecked(eth.inner_mut().payload_mut());
            arp.set_tpa(addr);
        }

        assert!(test_speculative_tpa(a.as_ref(), addr));

        // Let's also test for a very small buffer.
        let small = [0u8; 1];
        assert!(!test_speculative_tpa(small.as_ref(), addr));
    }
}
