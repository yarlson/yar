#include <setjmp.h>
#include <stddef.h>
#include <stdint.h>
#include <string.h>

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
        yar_gc_visit_range((const unsigned char *)&stack_marker,
                           (const unsigned char *)stack_top,
                           visitor,
                           context);
    }
}
