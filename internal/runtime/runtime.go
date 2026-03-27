package runtime

import _ "embed"

//go:embed runtime_source.txt
var source string

func Source() string {
	return source
}
