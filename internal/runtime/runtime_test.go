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
		"runtime failure: out of memory\\n",
	} {
		if !strings.Contains(Source(), want) {
			t.Fatalf("expected runtime source to contain %q", want)
		}
	}
}
