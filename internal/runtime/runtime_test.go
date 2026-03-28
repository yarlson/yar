package runtime

import (
	"strings"
	"testing"
)

func TestSourceIncludesMemoryHelpers(t *testing.T) {
	t.Parallel()

	for _, want := range []string{
		"void yar_trap_oom(void)",
		"void *yar_alloc(long long size)",
		"void *yar_alloc_zeroed(long long size)",
		"void yar_slice_index_check(long long index, long long len)",
		"void yar_slice_range_check(long long start, long long end, long long len)",
		"runtime failure: out of memory\\n",
		"runtime failure: slice index out of range\\n",
		"runtime failure: slice range out of bounds\\n",
	} {
		if !strings.Contains(Source(), want) {
			t.Fatalf("expected runtime source to contain %q", want)
		}
	}
}
