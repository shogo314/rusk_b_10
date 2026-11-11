#include <bit>
#include <cmath>
#include <string_view>

inline double rusk_parse_float(std::string_view text, Loc loc) {
    if (text == "nan") return std::numeric_limits<double>::quiet_NaN();
    if (text == "inf" || text == "+inf") return std::numeric_limits<double>::infinity();
    if (text == "-inf") return -std::numeric_limits<double>::infinity();
    if (text.starts_with('+')) {
        text.remove_prefix(1);
        if (text.starts_with('-')) fail(loc, "invalid Float");
    }
    auto significand = text.starts_with('-') ? text.substr(1) : text;
    if (significand.empty() ||
        (significand.front() != '.' && (significand.front() < '0' || significand.front() > '9'))) {
        fail(loc, "invalid Float");
    }
    double value;
    auto end = text.data() + text.size();
    auto result = std::from_chars(text.data(), end, value, std::chars_format::general);
    if (result.ec == std::errc::result_out_of_range) fail(loc, "Float out of range");
    if (result.ec != std::errc{} || result.ptr != end) fail(loc, "invalid Float");
    return value;
}

inline std::string rusk_float_text(double value, int precision, Loc loc) {
    if (std::isnan(value)) return "nan";
    if (std::isinf(value)) return std::signbit(value) ? "-inf" : "inf";
    // 309 digits before the point, 1000 after it, plus sign and point.
    char buffer[1311];
    auto end = buffer + sizeof(buffer);
    auto result = precision < 0
        ? std::to_chars(buffer, end, value)
        : std::to_chars(buffer, end, value, std::chars_format::fixed, precision);
    if (result.ec != std::errc{}) fail(loc, "Float formatting failed");
    return std::string(buffer, result.ptr);
}

inline std::string rusk_fixed_float(double value, std::int64_t precision, Loc loc) {
    if (precision < 0 || precision > 1000) fail(loc, "Float precision must be in 0..1000 inclusive");
    return rusk_float_text(value, static_cast<int>(precision), loc);
}

inline std::int64_t rusk_float_to_int(double value, Loc loc) {
    if (!(value >= -0x1p63 && value < 0x1p63)) fail(loc, "Float cannot be converted to Int");
    return static_cast<std::int64_t>(value);
}

inline std::uint64_t rusk_float_order_key(double value) {
    const std::uint64_t sign = UINT64_C(1) << 63;
    std::uint64_t bits = std::bit_cast<std::uint64_t>(value);
    return (bits & sign) ? ~bits : (bits ^ sign);
}
