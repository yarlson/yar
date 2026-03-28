package diag

import (
	"fmt"
	"strings"
	"yar/internal/token"
)

type Diagnostic struct {
	Pos     token.Position
	Message string
}

type List struct {
	items []Diagnostic
}

func (l *List) Add(pos token.Position, format string, args ...any) {
	l.items = append(l.items, Diagnostic{
		Pos:     pos,
		Message: fmt.Sprintf(format, args...),
	})
}

func (l *List) Append(other []Diagnostic) {
	l.items = append(l.items, other...)
}

func (l *List) Items() []Diagnostic {
	out := make([]Diagnostic, len(l.items))
	copy(out, l.items)
	return out
}

func (l *List) Empty() bool {
	return len(l.items) == 0
}

func Format(path string, diagnostics []Diagnostic) string {
	var b strings.Builder
	for i, d := range diagnostics {
		if i > 0 {
			b.WriteByte('\n')
		}
		diagPath := path
		if d.Pos.File != "" {
			diagPath = d.Pos.File
		}
		fmt.Fprintf(&b, "%s:%d:%d: %s", diagPath, d.Pos.Line, d.Pos.Column, d.Message)
	}
	return b.String()
}
