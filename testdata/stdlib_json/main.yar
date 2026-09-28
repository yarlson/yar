package main

import "std/conv"
import "std/json"
import "std/strings"

fn main() !i32 {
    document := "{\"name\":\"Ada\",\"age\":36,\"tags\":[\"a\",\"b\"],\"ok\":true,\"none\":null,\"pi\":-1.5e3}"
    parsed := json.parse(document)?
    if json.as_str(json.get(parsed, "name")?)? != "Ada" {
        return 1
    }
    if json.as_i64(json.get(parsed, "age")?)? != 36 || json.as_i32(json.get(parsed, "age")?)? != 36 {
        return 2
    }
    tags := json.as_array(json.get(parsed, "tags")?)?
    if len(tags) != 2 || json.as_str(tags[1])? != "b" {
        return 3
    }
    if !json.as_bool(json.get(parsed, "ok")?)? || !json.is_null(json.get(parsed, "none")?) {
        return 4
    }
    if json.encode(parsed)? != document {
        return 5
    }
    if json.encode(json.parse(" [ 1 ,\n\t2 ]\r\n")?)? != "[1,2]" {
        return 6
    }

    escaped := json.parse("\"a\\\"b\\\\c\\/d\\n\\u00e9\\ud83d\\ude00\"")?
    expected := "a\"b\\c/d\n" + bytes([]i32{195, 169, 240, 159, 152, 128})
    if json.as_str(escaped)? != expected {
        return 7
    }
    control := json.Value.String("q\"b\\\n\t" + conv.byte_to_str(1) + bytes([]i32{195, 169}))
    if json.encode(control)? != "\"q\\\"b\\\\\\n\\t\\u0001" + bytes([]i32{195, 169}) + "\"" {
        return 8
    }

    for i := 0; i < len(invalid_documents()); i += 1 {
        if !parse_fails(invalid_documents()[i], json.InvalidJSON) {
            print("accepted invalid document " + to_str(i) + "\n")
            return 9
        }
    }
    if !parse_fails("{\"a\":1,\"a\":2}", json.DuplicateName) {
        return 10
    }
    if !parse_fails(strings.repeat("[", 129) + strings.repeat("]", 129), json.TooDeep) {
        return 11
    }
    if len(json.encode(json.parse(strings.repeat("[", 128) + strings.repeat("]", 128))?)?) != 256 {
        return 12
    }
    if len(json.encode(nested_arrays(128))?) != 256 || !encode_fails(nested_arrays(129), json.TooDeep) {
        return 19
    }

    if !encode_fails(json.Value.Number("1e"), json.InvalidNumber) ||
        !encode_fails(json.Value.String(bytes([]i32{255})), json.InvalidString) {
        return 13
    }
    duplicate := json.Value.Object([]json.Member{
        json.Member{name: "a", value: json.Value.Null},
        json.Member{name: "a", value: json.Value.Null},
    })
    if !encode_fails(duplicate, json.DuplicateName) {
        return 14
    }

    built := json.Value.Object([]json.Member{
        json.Member{name: "id", value: json.int(conv.to_i64(-42))},
        json.Member{name: "ratio", value: json.number("1.5")?},
        json.Member{name: "items", value: json.Value.Array([]json.Value{json.Value.Bool(false)})},
    })
    if json.encode(built)? != "{\"id\":-42,\"ratio\":1.5,\"items\":[false]}" {
        return 15
    }
    if !number_rejected("abc") || !number_rejected("01") {
        return 16
    }

    if !get_fails(json.int(conv.to_i64(1)), "a", json.TypeMismatch) ||
        !get_fails(parsed, "missing", json.NotFound) {
        return 17
    }
    if !conversion_fails(json.get(parsed, "pi")?) ||
        !conversion_fails(json.parse("3000000000")?) ||
        !conversion_fails(json.parse("9223372036854775808")?) {
        return 18
    }

    print("json ok\n")
    return 0
}

fn invalid_documents() []str {
    return []str{
        "",
        "{",
        "[1,]",
        "{\"a\":1,}",
        "{a:1}",
        "01",
        "1.",
        "-",
        "1e+",
        "tru",
        "nul",
        "\"\\x\"",
        "\"\\ud800\"",
        "\"\\udc00\"",
        "\"\\u12\"",
        "[1] x",
        "\"a\nb\"",
        "\"" + bytes([]i32{255}) + "\"",
        "\"unterminated",
    }
}

fn nested_arrays(depth i32) json.Value {
    value := json.Value.Array([]json.Value{})
    for i := 1; i < depth; i += 1 {
        value = json.Value.Array([]json.Value{value})
    }
    return value
}

fn bytes(values []i32) str {
    out := ""
    for i := 0; i < len(values); i += 1 {
        out = out + conv.byte_to_str(values[i])
    }
    return out
}

fn parse_fails(text str, expected error) bool {
    value := json.parse(text) or |err| {
        return err == expected
    }
    _ := value
    return false
}

fn encode_fails(value json.Value, expected error) bool {
    text := json.encode(value) or |err| {
        return err == expected
    }
    _ := text
    return false
}

fn number_rejected(text str) bool {
    value := json.number(text) or |err| {
        return err == json.InvalidNumber
    }
    _ := value
    return false
}

fn get_fails(object json.Value, name str, expected error) bool {
    value := json.get(object, name) or |err| {
        return err == expected
    }
    _ := value
    return false
}

fn conversion_fails(value json.Value) bool {
    number := json.as_i32(value) or |err| {
        return err == json.OutOfRange
    }
    _ := number
    return false
}
