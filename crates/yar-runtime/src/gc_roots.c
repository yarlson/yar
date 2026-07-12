#include <setjmp.h>
#include <stddef.h>
#include <stdint.h>
#include <string.h>

#ifdef _WIN32
#include <windows.h>
#endif

typedef void (*yar_gc_root_visitor)(uintptr_t candidate, void *context);

static void yar_gc_visit_range(const unsigned char *start,
                               const unsigned char *end,
                               yar_gc_root_visitor visitor,
                               void *context) {
    uintptr_t low = (uintptr_t)start;
    uintptr_t high = (uintptr_t)end;
    if (low > high) {
        uintptr_t swap = low;
        low = high;
        high = swap;
    }

    if (high - low < sizeof(uintptr_t)) {
        return;
    }
    uintptr_t last = high - sizeof(uintptr_t);
    for (uintptr_t cursor = low;; cursor++) {
        uintptr_t candidate = 0;
        memcpy(&candidate, (const void *)cursor, sizeof(candidate));
        if (candidate != 0) {
            visitor(candidate, context);
        }
        if (cursor == last) {
            break;
        }
    }
}

#ifdef _WIN32
static void yar_gc_visit_stack_range(const unsigned char *start,
                                     const unsigned char *end,
                                     yar_gc_root_visitor visitor,
                                     void *context) {
    uintptr_t low = (uintptr_t)start;
    uintptr_t high = (uintptr_t)end;
    if (low > high) {
        uintptr_t swap = low;
        low = high;
        high = swap;
    }

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
            yar_gc_visit_range((const unsigned char *)readable_start,
                               (const unsigned char *)readable_end,
                               visitor,
                               context);
        }
        cursor = region_end;
    }
}
#else
static void yar_gc_visit_stack_range(const unsigned char *start,
                                     const unsigned char *end,
                                     yar_gc_root_visitor visitor,
                                     void *context) {
    yar_gc_visit_range(start, end, visitor, context);
}
#endif

void yar_gc_visit_stack_and_registers(const void *stack_top,
                                      yar_gc_root_visitor visitor,
                                      void *context) {
    jmp_buf registers;
    volatile unsigned char stack_marker = 0;

    if (stack_top == NULL || visitor == NULL) {
        return;
    }

    if (setjmp(registers) == 0) {
        yar_gc_visit_range((const unsigned char *)&registers,
                           (const unsigned char *)&registers + sizeof(registers),
                           visitor,
                           context);
        yar_gc_visit_stack_range((const unsigned char *)&stack_marker,
                                 (const unsigned char *)stack_top,
                                 visitor,
                                 context);
    }
}
