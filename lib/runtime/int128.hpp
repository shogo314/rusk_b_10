#include <string_view>

using RuskI128 = __int128;
using RuskU128 = unsigned __int128;

inline constexpr RuskU128 rusk_i128_sign_bit = static_cast<RuskU128>(1) << 127;
inline constexpr RuskI128 rusk_i128_max = static_cast<RuskI128>(rusk_i128_sign_bit - 1);
inline constexpr RuskI128 rusk_i128_min = -rusk_i128_max - 1;

inline RuskI128 rusk_add_i128(RuskI128 a, RuskI128 b, Loc loc) {
    RuskI128 result;
    if (__builtin_add_overflow(a, b, &result)) fail(loc, "Int128 overflow");
    return result;
}

inline RuskI128 rusk_sub_i128(RuskI128 a, RuskI128 b, Loc loc) {
    RuskI128 result;
    if (__builtin_sub_overflow(a, b, &result)) fail(loc, "Int128 overflow");
    return result;
}

inline RuskI128 rusk_mul_i128(RuskI128 a, RuskI128 b, Loc loc) {
    RuskI128 result;
    if (__builtin_mul_overflow(a, b, &result)) fail(loc, "Int128 overflow");
    return result;
}

inline RuskI128 rusk_div_i128(RuskI128 a, RuskI128 b, Loc loc) {
    if (b == 0) fail(loc, "division by zero");
    if (a == rusk_i128_min && b == -1) fail(loc, "Int128 overflow");
    RuskI128 q = a / b;
    RuskI128 r = a % b;
    return r != 0 && ((r < 0) != (b < 0)) ? q - 1 : q;
}

inline RuskI128 rusk_mod_i128(RuskI128 a, RuskI128 b, Loc loc) {
    if (b == 0) fail(loc, "division by zero");
    if (b == -1) return 0;
    RuskI128 r = a % b;
    return r != 0 && ((r < 0) != (b < 0)) ? r + b : r;
}

inline RuskI128 rusk_parse_i128(std::string_view text, Loc loc) {
    bool negative = text.starts_with('-');
    if (negative || text.starts_with('+')) text.remove_prefix(1);
    if (text.empty()) fail(loc, "invalid Int128");
    RuskU128 value = 0;
    const RuskU128 limit = negative ? rusk_i128_sign_bit : rusk_i128_sign_bit - 1;
    for (char c : text) {
        if (c < '0' || c > '9') fail(loc, "invalid Int128");
        unsigned digit = c - '0';
        if (value > (limit - digit) / 10) fail(loc, "Int128 out of range");
        value = value * 10 + digit;
    }
    if (value == rusk_i128_sign_bit) return rusk_i128_min;
    return negative ? -static_cast<RuskI128>(value) : static_cast<RuskI128>(value);
}

inline std::string rusk_i128_text(RuskI128 value) {
    char buffer[40]; // 39 decimal digits and an optional minus sign.
    char* end = buffer + sizeof(buffer);
    char* position = end;
    RuskU128 magnitude = static_cast<RuskU128>(value);
    if (value < 0) magnitude = 0 - magnitude;
    do {
        *--position = static_cast<char>('0' + magnitude % 10);
        magnitude /= 10;
    } while (magnitude != 0);
    if (value < 0) *--position = '-';
    return std::string(position, end);
}

inline RuskI128 rusk_float_to_i128(double value, Loc loc) {
    if (!(value >= -0x1p127 && value < 0x1p127)) fail(loc, "Float cannot be converted to Int128");
    return static_cast<RuskI128>(value);
}
