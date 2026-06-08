package utf8

pub fn decode(s str, off i32) !i32 {
    if off < 0 || off >= len(s) {
        return error.OutOfRange
    }
    b0 := s[off]
    if b0 < 128 {
        return b0
    }
    if b0 < 192 || b0 > 244 {
        return error.InvalidUTF8
    }
    if b0 < 224 {
        if off + 1 >= len(s) {
            return error.InvalidUTF8
        }
        b1 := s[off + 1]
        if b1 < 128 || b1 > 191 {
            return error.InvalidUTF8
        }
        r := (b0 - 192) * 64 + (b1 - 128)
        if r < 128 {
            return error.InvalidUTF8
        }
        return r
    }
    if b0 < 240 {
        if off + 2 >= len(s) {
            return error.InvalidUTF8
        }
        b1 := s[off + 1]
        b2 := s[off + 2]
        if b1 < 128 || b1 > 191 || b2 < 128 || b2 > 191 {
            return error.InvalidUTF8
        }
        r := (b0 - 224) * 4096 + (b1 - 128) * 64 + (b2 - 128)
        if r < 2048 {
            return error.InvalidUTF8
        }
        if r >= 55296 && r <= 57343 {
            return error.InvalidUTF8
        }
        return r
    }
    if off + 3 >= len(s) {
        return error.InvalidUTF8
    }
    b1 := s[off + 1]
    b2 := s[off + 2]
    b3 := s[off + 3]
    if b1 < 128 || b1 > 191 || b2 < 128 || b2 > 191 || b3 < 128 || b3 > 191 {
        return error.InvalidUTF8
    }
    r := (b0 - 240) * 262144 + (b1 - 128) * 4096 + (b2 - 128) * 64 + (b3 - 128)
    if r < 65536 || r > 1114111 {
        return error.InvalidUTF8
    }
    return r
}

pub fn width(s str, off i32) !i32 {
    if off < 0 || off >= len(s) {
        return error.OutOfRange
    }
    b0 := s[off]
    if b0 < 128 {
        return 1
    }
    if b0 < 192 || b0 > 244 {
        return error.InvalidUTF8
    }
    if b0 < 224 {
        if off + 1 >= len(s) {
            return error.InvalidUTF8
        }
        b1 := s[off + 1]
        if b1 < 128 || b1 > 191 {
            return error.InvalidUTF8
        }
        r := (b0 - 192) * 64 + (b1 - 128)
        if r < 128 {
            return error.InvalidUTF8
        }
        return 2
    }
    if b0 < 240 {
        if off + 2 >= len(s) {
            return error.InvalidUTF8
        }
        b1 := s[off + 1]
        b2 := s[off + 2]
        if b1 < 128 || b1 > 191 || b2 < 128 || b2 > 191 {
            return error.InvalidUTF8
        }
        r := (b0 - 224) * 4096 + (b1 - 128) * 64 + (b2 - 128)
        if r < 2048 {
            return error.InvalidUTF8
        }
        if r >= 55296 && r <= 57343 {
            return error.InvalidUTF8
        }
        return 3
    }
    if off + 3 >= len(s) {
        return error.InvalidUTF8
    }
    b1 := s[off + 1]
    b2 := s[off + 2]
    b3 := s[off + 3]
    if b1 < 128 || b1 > 191 || b2 < 128 || b2 > 191 || b3 < 128 || b3 > 191 {
        return error.InvalidUTF8
    }
    r := (b0 - 240) * 262144 + (b1 - 128) * 4096 + (b2 - 128) * 64 + (b3 - 128)
    if r < 65536 || r > 1114111 {
        return error.InvalidUTF8
    }
    return 4
}

pub fn is_letter(r i32) bool {
    if r >= 65 && r <= 90 {
        return true
    }
    if r >= 97 && r <= 122 {
        return true
    }
    if r == 95 {
        return true
    }
    if r < 128 {
        return false
    }
    if r >= 192 && r <= 687 {
        return true
    }
    if r >= 880 && r <= 1154 {
        return true
    }
    if r >= 1162 && r <= 1327 {
        return true
    }
    if r >= 1329 && r <= 1415 {
        return true
    }
    if r >= 1488 && r <= 1514 {
        return true
    }
    if r >= 1519 && r <= 1524 {
        return true
    }
    if r >= 1568 && r <= 1610 {
        return true
    }
    if r >= 1646 && r <= 1647 {
        return true
    }
    if r >= 1649 && r <= 1747 {
        return true
    }
    if r >= 1774 && r <= 1775 {
        return true
    }
    if r >= 1786 && r <= 1788 {
        return true
    }
    if r >= 2308 && r <= 2361 {
        return true
    }
    if r >= 2365 && r <= 2384 {
        return true
    }
    if r >= 2392 && r <= 2401 {
        return true
    }
    if r >= 4256 && r <= 4293 {
        return true
    }
    if r >= 4304 && r <= 4346 {
        return true
    }
    if r >= 4352 && r <= 4607 {
        return true
    }
    if r >= 4608 && r <= 5017 {
        return true
    }
    if r >= 5024 && r <= 5117 {
        return true
    }
    if r >= 5121 && r <= 5759 {
        return true
    }
    if r >= 5761 && r <= 5786 {
        return true
    }
    if r >= 5792 && r <= 5866 {
        return true
    }
    if r >= 5870 && r <= 5880 {
        return true
    }
    if r >= 5888 && r <= 5905 {
        return true
    }
    if r >= 5920 && r <= 5937 {
        return true
    }
    if r >= 5952 && r <= 5969 {
        return true
    }
    if r >= 5984 && r <= 5996 {
        return true
    }
    if r >= 5998 && r <= 6000 {
        return true
    }
    if r >= 6016 && r <= 6067 {
        return true
    }
    if r >= 6176 && r <= 6263 {
        return true
    }
    if r >= 6272 && r <= 6312 {
        return true
    }
    if r >= 7680 && r <= 7957 {
        return true
    }
    if r >= 7960 && r <= 7965 {
        return true
    }
    if r >= 7968 && r <= 8005 {
        return true
    }
    if r >= 8008 && r <= 8013 {
        return true
    }
    if r >= 8016 && r <= 8023 {
        return true
    }
    if r >= 8025 && r <= 8031 {
        return true
    }
    if r >= 8064 && r <= 8116 {
        return true
    }
    if r >= 8118 && r <= 8124 {
        return true
    }
    if r >= 8130 && r <= 8132 {
        return true
    }
    if r >= 8134 && r <= 8140 {
        return true
    }
    if r >= 8144 && r <= 8147 {
        return true
    }
    if r >= 8150 && r <= 8155 {
        return true
    }
    if r >= 8160 && r <= 8172 {
        return true
    }
    if r >= 8178 && r <= 8180 {
        return true
    }
    if r >= 8182 && r <= 8188 {
        return true
    }
    if r >= 11264 && r <= 11492 {
        return true
    }
    if r >= 19968 && r <= 40959 {
        return true
    }
    if r >= 44032 && r <= 55203 {
        return true
    }
    if r >= 63744 && r <= 64217 {
        return true
    }
    return false
}

pub fn is_digit(r i32) bool {
    return r >= 48 && r <= 57
}

pub fn is_space(r i32) bool {
    if r == 32 {
        return true
    }
    if r == 9 {
        return true
    }
    if r == 10 {
        return true
    }
    if r == 13 {
        return true
    }
    if r == 12 {
        return true
    }
    if r == 11 {
        return true
    }
    if r == 133 {
        return true
    }
    if r == 160 {
        return true
    }
    if r == 5760 {
        return true
    }
    if r >= 8192 && r <= 8202 {
        return true
    }
    if r == 8232 {
        return true
    }
    if r == 8233 {
        return true
    }
    if r == 8239 {
        return true
    }
    if r == 8287 {
        return true
    }
    if r == 12288 {
        return true
    }
    return false
}
