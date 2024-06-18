use crate::{
    error::{Error, Result},
    Mbuf,
};
use std::{
    ffi::{c_char, CString},
    os::raw::c_void,
    ptr::NonNull,
    sync::Arc,
};

use arrayvec::ArrayVec;
use bitflags::bitflags;
use rpkt_dpdk_sys as ffi;

bitflags! {
    #[derive(Clone, Debug)]
    pub struct RingFlags: u32 {
        /// The default enqueue is "single-producer".
        const SP_ENQ      = 0x0001;
        /// The default dequeue is "single-consumer".
        const SC_DEQ      = 0x0002;
        ///  Ring holds exactly requested number of entries.
        const EXACT_SZ    = 0x0004;
        /// The default enqueue is "MP RTS".
        const MP_RTS_ENQ  = 0x0008;
        /// The default dequeue is "MC RTS".
        const MC_RTS_DEQ  = 0x0010;
        /// The default enqueue is "MP HTS".
        const MP_HTS_ENQ  = 0x0020;
        /// The default dequeue is "MC HTS".
        const MC_HTS_DEQ  = 0x0040;
    }
}

/// Ring size mask
const RTE_RING_SZ_MASK: u32 = 0x7fffffff;

#[derive(Clone, Debug)]
pub struct RingConf {
    ///   The size of the ring (must be a power of 2,
    ///   unless RING_F_EXACT_SZ is set in flags).
    pub count: u32,
    /// The *socket_id* argument is the socket identifier in case of
    ///   NUMA. The value can be *SOCKET_ID_ANY* if there is no NUMA
    ///   constraint for the reserved zone.
    pub socket_id: u32,

    ///   An OR of the following:
    ///   - One of mutually exclusive flags that define producer behavior:
    ///      - RING_F_SP_ENQ: If this flag is set, the default behavior when
    ///        using ``rte_ring_enqueue()`` or ``rte_ring_enqueue_bulk()``
    ///        is "single-producer".
    ///      - RING_F_MP_RTS_ENQ: If this flag is set, the default behavior when
    ///        using ``rte_ring_enqueue()`` or ``rte_ring_enqueue_bulk()``
    ///        is "multi-producer RTS mode".
    ///      - RING_F_MP_HTS_ENQ: If this flag is set, the default behavior when
    ///        using ``rte_ring_enqueue()`` or ``rte_ring_enqueue_bulk()``
    ///        is "multi-producer HTS mode".
    ///     If none of these flags is set, then default "multi-producer"
    ///     behavior is selected.
    ///   - One of mutually exclusive flags that define consumer behavior:
    ///      - RING_F_SC_DEQ: If this flag is set, the default behavior when
    ///        using ``rte_ring_dequeue()`` or ``rte_ring_dequeue_bulk()``
    ///        is "single-consumer". Otherwise, it is "multi-consumers".
    ///      - RING_F_MC_RTS_DEQ: If this flag is set, the default behavior when
    ///        using ``rte_ring_dequeue()`` or ``rte_ring_dequeue_bulk()``
    ///        is "multi-consumer RTS mode".
    ///      - RING_F_MC_HTS_DEQ: If this flag is set, the default behavior when
    ///        using ``rte_ring_dequeue()`` or ``rte_ring_dequeue_bulk()``
    ///        is "multi-consumer HTS mode".
    ///     If none of these flags is set, then default "multi-consumer"
    ///     behavior is selected.
    ///   - RING_F_EXACT_SZ: If this flag is set, the ring will hold exactly the
    ///     requested number of entries, and the requested size will be rounded up
    ///     to the next power of two, but the usable space will be exactly that
    ///     requested. Worst case, if a power-of-2 size is requested, half the
    ///     ring space will be wasted.
    ///     Without this flag set, the ring size requested must be a power of 2,
    ///     and the usable space will be that size - 1.
    pub flag: RingFlags,
}

#[derive(Clone)]
pub struct Ring {
    ptr: NonNull<ffi::rte_ring>,
    counter: Arc<()>,
}

unsafe impl Send for Ring {}
unsafe impl Sync for Ring {}

impl Ring {
    pub(crate) fn try_create(name: String, conf: &RingConf) -> Result<Self> {
        let err = Error::service_err("invalid ring config");
        let socket_id = i32::try_from(conf.socket_id).map_err(|_| err)?;

        let cname = CString::new(name).map_err(|_| Error::service_err("invalid ring name"))?;

        let raw = unsafe {
            ffi::rte_ring_create(
                cname.as_bytes_with_nul().as_ptr() as *const c_char,
                conf.count,
                socket_id,
                conf.flag.bits(),
            )
        };

        let ptr = NonNull::new(raw).ok_or_else(|| {
            Error::ffi_err(unsafe { ffi::rte_errno_() }, "failed to allocate ring")
        })?;
        Ok(Self {
            ptr,
            counter: Arc::new(()),
        })
    }

    #[inline]
    pub unsafe fn enqueue_burst<const N: usize>(
        &self,
        batch: &mut ArrayVec<Mbuf, N>,
    ) -> Result<()> {
        let mbufs = std::mem::transmute::<*mut Mbuf, *mut *mut ffi::rte_mbuf>(batch.as_mut_ptr());

        let res = ffi::rte_ring_enqueue_burst_(
            self.ptr.as_ptr(),
            mbufs as *const *mut c_void,
            batch.len() as u32,
            std::ptr::null_mut(),
        );
        if res != 0 {
            return Error::ffi_err(res as i32, "fail to enqueue burst").to_err();
        }
        Ok(())
    }

    #[inline]
    pub unsafe fn dequeue_burst<const N: usize>(&self) -> Result<&ArrayVec<Mbuf, N>> {
        todo!()
    }

    pub fn as_ptr(&self) -> *const ffi::rte_ring {
        self.ptr.as_ptr()
    }
}

impl Drop for Ring {
    fn drop(&mut self) {
        unsafe {
            ffi::rte_ring_free(self.ptr.as_ptr());
        }
    }
}
