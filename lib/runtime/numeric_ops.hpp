template<class Signed, class Unsigned>
inline Signed rusk_shift(Signed value, std::int64_t amount, bool left) {
    std::uint64_t magnitude = static_cast<std::uint64_t>(amount);
    if (amount < 0) {
        left = !left;
        magnitude = 0 - magnitude;
    }
    constexpr unsigned width = sizeof(Signed) * 8;
    if (magnitude >= width) return left || value >= 0 ? Signed{0} : Signed{-1};
    if (magnitude == 0) return value;

    Unsigned bits = std::bit_cast<Unsigned>(value);
    if (left) {
        bits <<= magnitude;
    } else {
        bits >>= magnitude;
        if (value < 0) bits |= (~Unsigned{0}) << (width - magnitude);
    }
    return std::bit_cast<Signed>(bits);
}

template<class Integer, class Multiply>
inline Integer rusk_integer_pow(Integer base, std::int64_t exponent, Loc loc, Multiply multiply) {
    if (exponent < 0) fail(loc, "negative integer exponent");
    Integer result = 1;
    while (exponent != 0) {
        if (exponent & 1) result = multiply(result, base, loc);
        exponent >>= 1;
        if (exponent != 0) base = multiply(base, base, loc);
    }
    return result;
}

inline std::int64_t rusk_pow_i64(std::int64_t base, std::int64_t exponent, Loc loc) {
    return rusk_integer_pow(base, exponent, loc, [](std::int64_t a, std::int64_t b, Loc loc) {
        return checked_int(static_cast<__int128>(a) * b, loc);
    });
}

inline RuskI128 rusk_pow_i128(RuskI128 base, std::int64_t exponent, Loc loc) {
    return rusk_integer_pow(base, exponent, loc, rusk_mul_i128);
}
