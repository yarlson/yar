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

func TestSourceIncludesFilesystemHelpers(t *testing.T) {
	t.Parallel()

	for _, want := range []string{
		"yar_fs_read_file(yar_str path, yar_str *out)",
		"yar_fs_write_file(yar_str path, yar_str data)",
		"yar_fs_read_dir(yar_str path, yar_slice *out)",
		"yar_fs_stat(yar_str path, int32_t *kind_out)",
		"yar_fs_mkdir_all(yar_str path)",
		"yar_fs_remove_all(yar_str path)",
		"yar_fs_temp_dir(yar_str prefix, yar_str *out)",
	} {
		if !strings.Contains(Source(), want) {
			t.Fatalf("expected runtime source to contain %q", want)
		}
	}
}

func TestSourceIncludesProcessHelpers(t *testing.T) {
	t.Parallel()

	for _, want := range []string{
		"void yar_set_args(int32_t argc, char **argv)",
		"void yar_process_args(yar_slice *out)",
		"int32_t yar_process_run(const yar_slice *argv, yar_process_result *out)",
		"int32_t yar_process_run_inherit(const yar_slice *argv, int32_t *exit_code_out)",
		"int32_t yar_env_lookup(yar_str name, yar_str *out)",
		"void yar_eprint(const char *data, long long len)",
	} {
		if !strings.Contains(Source(), want) {
			t.Fatalf("expected runtime source to contain %q", want)
		}
	}
}

func TestSourceIncludesMapKeyHelpers(t *testing.T) {
	t.Parallel()

	for _, want := range []string{
		"int32_t yar_map_len(void *map_ptr)",
		"yar_slice yar_map_keys(void *map_ptr)",
	} {
		if !strings.Contains(Source(), want) {
			t.Fatalf("expected runtime source to contain %q", want)
		}
	}
}
