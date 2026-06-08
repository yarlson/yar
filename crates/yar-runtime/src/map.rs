use std::ptr;

use crate::{YarSlice, YarStr};

const INIT_CAP: i32 = 8;
const LOAD_NUM: i64 = 3;
const LOAD_DEN: i64 = 4;

const KEY_STR: i32 = 3;

#[repr(C)]
struct RuntimeMap {
    entries: *mut u8,
    count: i32,
    cap: i32,
    key_size: i32,
    value_size: i32,
    bucket_size: i32,
    key_kind: i32,
}

pub(crate) fn new(key_kind: i32, key_size: i32, value_size: i32) -> *mut u8 {
    if key_size <= 0 || value_size <= 0 {
        super::runtime_fail(b"runtime failure: invalid map layout\n");
    }

    let Some(bucket_size) = 1_i32
        .checked_add(key_size)
        .and_then(|size| size.checked_add(value_size))
    else {
        super::runtime_fail(b"runtime failure: invalid map layout\n");
    };

    let map = super::yar_alloc_zeroed(size_of::<RuntimeMap>() as i64).cast::<RuntimeMap>();
    // SAFETY: map points to writable runtime-managed memory sized for RuntimeMap.
    unsafe {
        ptr::write(
            map,
            RuntimeMap {
                entries: alloc_entries(INIT_CAP, bucket_size),
                count: 0,
                cap: INIT_CAP,
                key_size,
                value_size,
                bucket_size,
                key_kind,
            },
        );
    }
    map.cast::<u8>()
}

pub(crate) fn set(map_ptr: *mut u8, key: *const u8, value: *const u8) {
    let map = checked_map(map_ptr);
    if key.is_null() || value.is_null() {
        super::runtime_fail(b"runtime failure: invalid map access\n");
    }

    // SAFETY: map/key/value are runtime ABI pointers validated above.
    unsafe {
        if i64::from((*map).count + 1) * LOAD_DEN > i64::from((*map).cap) * LOAD_NUM {
            grow(map);
        }
        insert_existing(map, key, value);
    }
}

pub(crate) fn get(map_ptr: *mut u8, key: *const u8, value_out: *mut u8) -> i32 {
    let map = checked_map(map_ptr);
    if key.is_null() || value_out.is_null() {
        super::runtime_fail(b"runtime failure: invalid map access\n");
    }

    // SAFETY: map/key/value_out are runtime ABI pointers validated above.
    unsafe {
        let (bucket, found) = find_slot(map, key);
        if !found {
            return 0;
        }
        ptr::copy_nonoverlapping(
            bucket.add(1 + (*map).key_size as usize),
            value_out,
            (*map).value_size as usize,
        );
    }
    1
}

pub(crate) fn has(map_ptr: *mut u8, key: *const u8) -> i32 {
    let map = checked_map(map_ptr);
    if key.is_null() {
        super::runtime_fail(b"runtime failure: invalid map access\n");
    }

    // SAFETY: map/key are runtime ABI pointers validated above.
    let found = unsafe {
        let (_, found) = find_slot(map, key);
        found
    };
    i32::from(found)
}

pub(crate) fn delete(map_ptr: *mut u8, key: *const u8) {
    let map = checked_map(map_ptr);
    if key.is_null() {
        super::runtime_fail(b"runtime failure: invalid map access\n");
    }

    // SAFETY: map/key are runtime ABI pointers validated above.
    unsafe {
        let (bucket, found) = find_slot(map, key);
        if !found {
            return;
        }

        *bucket = 0;
        (*map).count -= 1;

        let mask = (*map).cap - 1;
        let mut idx = (bucket_index(map, bucket) + 1) & mask;
        loop {
            let next = bucket_at(map, idx);
            if *next == 0 {
                break;
            }

            let saved = std::slice::from_raw_parts(next, (*map).bucket_size as usize).to_vec();
            *next = 0;
            (*map).count -= 1;
            insert_existing(
                map,
                saved.as_ptr().add(1),
                saved.as_ptr().add(1 + (*map).key_size as usize),
            );
            idx = (idx + 1) & mask;
        }
    }
}

pub(crate) fn len(map_ptr: *mut u8) -> i32 {
    let map = checked_map(map_ptr);
    // SAFETY: checked_map returns a non-null RuntimeMap pointer.
    unsafe { (*map).count }
}

pub(crate) fn keys(map_ptr: *mut u8) -> YarSlice {
    let map = checked_map(map_ptr);
    // SAFETY: checked_map returns a non-null RuntimeMap pointer.
    unsafe {
        if (*map).count == 0 {
            return YarSlice {
                ptr: ptr::null_mut(),
                len: 0,
                cap: 0,
            };
        }

        let total_size = i64::from((*map).count) * i64::from((*map).key_size);
        let out = super::yar_alloc(total_size);
        let mut copied = 0_i32;
        for idx in 0..(*map).cap {
            let bucket = bucket_at(map, idx);
            if *bucket == 0 {
                continue;
            }
            ptr::copy_nonoverlapping(
                bucket.add(1),
                out.add((copied * (*map).key_size) as usize),
                (*map).key_size as usize,
            );
            copied += 1;
        }

        YarSlice {
            ptr: out,
            len: copied,
            cap: copied,
        }
    }
}

fn checked_map(map_ptr: *mut u8) -> *mut RuntimeMap {
    if map_ptr.is_null() {
        super::runtime_fail(b"runtime failure: invalid map access\n");
    }
    map_ptr.cast::<RuntimeMap>()
}

fn alloc_entries(cap: i32, bucket_size: i32) -> *mut u8 {
    let total_size = i64::from(cap) * i64::from(bucket_size);
    super::yar_alloc_zeroed(total_size)
}

unsafe fn grow(map: *mut RuntimeMap) {
    let old_cap = unsafe { (*map).cap };
    let old_entries = unsafe { (*map).entries };
    let new_cap = old_cap * 2;

    unsafe {
        (*map).cap = new_cap;
        (*map).entries = alloc_entries(new_cap, (*map).bucket_size);
        (*map).count = 0;

        for idx in 0..old_cap {
            let bucket = old_entries.add((idx * (*map).bucket_size) as usize);
            if *bucket == 0 {
                continue;
            }
            insert_existing(map, bucket.add(1), bucket.add(1 + (*map).key_size as usize));
        }
    }
}

unsafe fn insert_existing(map: *mut RuntimeMap, key: *const u8, value: *const u8) {
    let (bucket, found) = unsafe { find_slot(map, key) };
    unsafe {
        if !found {
            *bucket = 1;
            ptr::copy_nonoverlapping(key, bucket.add(1), (*map).key_size as usize);
            (*map).count += 1;
        }
        ptr::copy_nonoverlapping(
            value,
            bucket.add(1 + (*map).key_size as usize),
            (*map).value_size as usize,
        );
    }
}

unsafe fn find_slot(map: *mut RuntimeMap, key: *const u8) -> (*mut u8, bool) {
    let hash = unsafe { hash_key(map, key) };
    let mask = unsafe { (*map).cap - 1 };
    let mut idx = (hash & mask as u64) as i32;

    loop {
        let bucket = unsafe { bucket_at(map, idx) };
        unsafe {
            if *bucket == 0 {
                return (bucket, false);
            }
            if keys_equal(map, bucket.add(1), key) {
                return (bucket, true);
            }
        }
        idx = (idx + 1) & mask;
    }
}

unsafe fn bucket_at(map: *const RuntimeMap, idx: i32) -> *mut u8 {
    unsafe { (*map).entries.add((idx * (*map).bucket_size) as usize) }
}

unsafe fn bucket_index(map: *const RuntimeMap, bucket: *const u8) -> i32 {
    unsafe { (bucket.offset_from((*map).entries) / (*map).bucket_size as isize) as i32 }
}

unsafe fn hash_key(map: *const RuntimeMap, key: *const u8) -> u64 {
    let mut hash = 14_695_981_039_346_656_037_u64;
    let bytes = unsafe {
        if (*map).key_kind == KEY_STR {
            let key = ptr::read_unaligned(key.cast::<YarStr>());
            raw_bytes(key.ptr.cast_const(), key.len)
        } else {
            raw_bytes(key, i64::from((*map).key_size))
        }
    };

    for byte in bytes {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(1_099_511_628_211);
    }
    hash
}

unsafe fn keys_equal(map: *const RuntimeMap, a: *const u8, b: *const u8) -> bool {
    unsafe {
        if (*map).key_kind == KEY_STR {
            let a = ptr::read_unaligned(a.cast::<YarStr>());
            let b = ptr::read_unaligned(b.cast::<YarStr>());
            return a.len == b.len
                && raw_bytes(a.ptr.cast_const(), a.len) == raw_bytes(b.ptr.cast_const(), b.len);
        }

        raw_bytes(a, i64::from((*map).key_size)) == raw_bytes(b, i64::from((*map).key_size))
    }
}

unsafe fn raw_bytes<'a>(ptr: *const u8, len: i64) -> &'a [u8] {
    if len <= 0 {
        return &[];
    }
    if ptr.is_null() {
        super::runtime_fail(b"runtime failure: invalid map key\n");
    }
    unsafe { std::slice::from_raw_parts(ptr, len as usize) }
}
