package http

import "std/strings"
import "std/url"

pub struct Route {
    method str
    segments []Segment
    handler fn(Request) !Response
}

pub struct Router {
    routes []Route
}

enum Segment {
    Literal { text str }
    Param { name str }
    Rest { name str }
}

struct PathMatch {
    matched bool
    values []url.Param
}

struct RouteChoice {
    index i32
    values []url.Param
}

pub fn route(method str, pattern str, handler fn(Request) !Response) !Route {
    if !valid_token(method) {
        return error.InvalidArgument
    }
    return Route{method: method, segments: parse_pattern(pattern)?, handler: handler}
}

pub fn router(routes []Route) !Router {
    owned := []Route{}
    for i := 0; i < len(routes); i += 1 {
        for j := 0; j < i; j += 1 {
            if routes[i].method == routes[j].method &&
                same_shape(routes[i].segments, routes[j].segments) {
                return error.RouteConflict
            }
        }
        owned = append(owned, routes[i])
    }
    return Router{routes: owned}
}

pub fn (r Router) serve(req Request) !Response {
    path := req.path()
    if len(path) == 0 || path[0] != '/' {
        return response(404, "")
    }
    raw := strings.split(path[1:], "/")
    decoded := []str{}
    for i := 0; i < len(raw); i += 1 {
        segment := url.percent_decode(raw[i]) or |err| {
            return response(400, "")
        }
        decoded = append(decoded, segment)
    }

    choice := best_route(r.routes, req.method, raw, decoded)
    if choice.index < 0 && req.method == "HEAD" {
        choice = best_route(r.routes, "GET", raw, decoded)
    }
    if choice.index >= 0 {
        routed := req
        routed.path_values = choice.values
        handler := r.routes[choice.index].handler
        return handler(routed)
    }

    allowed := allowed_methods(r.routes, raw, decoded)
    if len(allowed) == 0 {
        return response(404, "")
    }
    return response(405, "")?.with_header("Allow", strings.join(allowed, ", "))
}

pub fn (r Request) path_value(name str) !str {
    for i := 0; i < len(r.path_values); i += 1 {
        if r.path_values[i].name == name {
            return r.path_values[i].value
        }
    }
    return error.PathValueNotFound
}

fn best_route(routes []Route, method str, raw []str, decoded []str) RouteChoice {
    best := RouteChoice{index: -1, values: []url.Param{}}
    for i := 0; i < len(routes); i += 1 {
        if routes[i].method != method {
            continue
        }
        found := match_segments(routes[i].segments, raw, decoded)
        if !found.matched {
            continue
        }
        if best.index < 0 || more_specific(routes[i].segments, routes[best.index].segments) {
            best = RouteChoice{index: i, values: found.values}
        }
    }
    return best
}

fn allowed_methods(routes []Route, raw []str, decoded []str) []str {
    allowed := []str{}
    for i := 0; i < len(routes); i += 1 {
        if !match_segments(routes[i].segments, raw, decoded).matched {
            continue
        }
        allowed = append_method(allowed, routes[i].method)
        if routes[i].method == "GET" {
            allowed = append_method(allowed, "HEAD")
        }
    }
    return allowed
}

fn append_method(methods []str, method str) []str {
    for i := 0; i < len(methods); i += 1 {
        if methods[i] == method {
            return methods
        }
    }
    return append(methods, method)
}

fn match_segments(segments []Segment, raw []str, decoded []str) PathMatch {
    none := PathMatch{matched: false, values: []url.Param{}}
    values := []url.Param{}
    for i := 0; i < len(segments); i += 1 {
        match segments[i] {
        case Segment.Rest(rest) {
            if i >= len(raw) {
                return none
            }
            value := url.percent_decode(strings.join(raw[i:], "/")) or |err| {
                return none
            }
            values = append(values, url.Param{name: rest.name, value: value})
            return PathMatch{matched: true, values: values}
        }
        case Segment.Param(param) {
            if i >= len(decoded) || len(decoded[i]) == 0 {
                return none
            }
            values = append(values, url.Param{name: param.name, value: decoded[i]})
        }
        case Segment.Literal(literal) {
            if i >= len(decoded) || decoded[i] != literal.text {
                return none
            }
        }
        }
    }
    if len(segments) != len(decoded) {
        return none
    }
    return PathMatch{matched: true, values: values}
}

fn more_specific(a []Segment, b []Segment) bool {
    for i := 0; i < len(a) && i < len(b); i += 1 {
        if segment_rank(a[i]) != segment_rank(b[i]) {
            return segment_rank(a[i]) > segment_rank(b[i])
        }
    }
    return false
}

fn segment_rank(segment Segment) i32 {
    match segment {
    case Segment.Literal(_) {
        return 2
    }
    case Segment.Param(_) {
        return 1
    }
    case Segment.Rest(_) {
        return 0
    }
    }
}

fn same_shape(a []Segment, b []Segment) bool {
    if len(a) != len(b) {
        return false
    }
    for i := 0; i < len(a); i += 1 {
        if segment_rank(a[i]) != segment_rank(b[i]) {
            return false
        }
        if segment_rank(a[i]) == 2 && literal_text(a[i]) != literal_text(b[i]) {
            return false
        }
    }
    return true
}

fn literal_text(segment Segment) str {
    match segment {
    case Segment.Literal(literal) {
        return literal.text
    }
    else {
        return ""
    }
    }
}

fn parse_pattern(pattern str) ![]Segment {
    if len(pattern) == 0 || pattern[0] != '/' {
        return error.InvalidArgument
    }
    parts := strings.split(pattern[1:], "/")
    segments := []Segment{}
    names := []str{}
    for i := 0; i < len(parts); i += 1 {
        part := parts[i]
        last := i + 1 == len(parts)
        if len(part) >= 2 && part[0] == '{' && part[len(part) - 1] == '}' {
            name := part[1:len(part) - 1]
            rest := strings.has_suffix(name, "...")
            if rest {
                name = name[0:len(name) - 3]
                if !last {
                    return error.InvalidArgument
                }
            }
            if !valid_param_name(name) || contains_name(names, name) {
                return error.InvalidArgument
            }
            names = append(names, name)
            if rest {
                segments = append(segments, Segment.Rest{name: name})
            } else {
                segments = append(segments, Segment.Param{name: name})
            }
        } else {
            if !valid_literal_segment(part) || (len(part) == 0 && !last) {
                return error.InvalidArgument
            }
            segments = append(segments, Segment.Literal{text: part})
        }
    }
    return segments
}

fn valid_literal_segment(value str) bool {
    for i := 0; i < len(value); i += 1 {
        if !uri_pchar_byte(value[i]) {
            return false
        }
    }
    return true
}

fn valid_param_name(value str) bool {
    if len(value) == 0 || (!alpha_byte(value[0]) && value[0] != '_') {
        return false
    }
    for i := 1; i < len(value); i += 1 {
        if !alpha_byte(value[i]) && !decimal_byte(value[i]) && value[i] != '_' {
            return false
        }
    }
    return true
}

fn contains_name(names []str, name str) bool {
    for i := 0; i < len(names); i += 1 {
        if names[i] == name {
            return true
        }
    }
    return false
}
