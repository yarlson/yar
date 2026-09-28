#if defined(_WIN32) && !defined(_WIN32_WINNT)
#define _WIN32_WINNT 0x0602
#endif

#include <setjmp.h>
#include <stddef.h>
#include <stdint.h>

#ifdef _WIN32
#include <windows.h>
#endif

typedef void (*yar_gc_spilled_fn)(void *context, void *stack_low);
typedef void (*yar_gc_range_fn)(uintptr_t low, uintptr_t high, void *context);

void yar_gc_spill_registers_and_call(yar_gc_spilled_fn fn, void *context) {
    jmp_buf registers;
#if defined(__GNUC__) || defined(__clang__)
    __builtin_unwind_init();
#endif
    if (setjmp(registers) == 0) {
        fn(context, (void *)&registers);
    }
}

void *yar_gc_thread_stack_top(void *marker) {
#ifdef _WIN32
    ULONG_PTR stack_low = 0;
    ULONG_PTR stack_high = 0;
    GetCurrentThreadStackLimits(&stack_low, &stack_high);
    (void)marker;
    return (void *)stack_high;
#else
    return marker;
#endif
}

void yar_gc_for_each_readable_range(uintptr_t low,
                                    uintptr_t high,
                                    yar_gc_range_fn fn,
                                    void *context) {
    if (low >= high) {
        return;
    }
#ifdef _WIN32
    uintptr_t cursor = low;
    while (cursor < high) {
        MEMORY_BASIC_INFORMATION region;
        if (VirtualQuery((const void *)cursor, &region, sizeof(region)) == 0) {
            return;
        }
        uintptr_t region_start = (uintptr_t)region.BaseAddress;
        uintptr_t region_end = region_start + region.RegionSize;
        if (region_end <= cursor) {
            return;
        }
        uintptr_t readable_start = cursor > region_start ? cursor : region_start;
        uintptr_t readable_end = high < region_end ? high : region_end;
        if (region.State == MEM_COMMIT &&
            (region.Protect & (PAGE_GUARD | PAGE_NOACCESS)) == 0) {
            fn(readable_start, readable_end, context);
        }
        cursor = region_end;
    }
#else
    fn(low, high, context);
#endif
}
