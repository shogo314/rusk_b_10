template<class I, class T, class Has, class Next, class Add>
Obj<T> rusk_sum_iter(const Obj<I>& it, Obj<T> total, Loc loc, Has has, Next next, Add add) {
    while (*has(it, loc)) {
        {
            auto value = next(it, loc);
            rusk_guard_iter(it, loc);
            total = add(total, value, loc);
        }
        rusk_guard_iter(it, loc);
    }
    rusk_guard_iter(it, loc);
    return total;
}

template<bool Maximum, bool TrackPosition, class I, class Has, class Next, class Less>
auto rusk_extreme(const Obj<I>& it, Loc loc, Has has, Next next, Less less) {
    struct Selection {
        decltype(next(it, loc)) value;
        std::int64_t index = 0;
    } best;
    std::int64_t index = -1;
    while (*has(it, loc)) {
        if constexpr (TrackPosition) {
            if (index == std::numeric_limits<std::int64_t>::max()) {
                fail(loc, "aggregation index overflow");
            }
            ++index;
        }
        {
            auto candidate = next(it, loc);
            rusk_guard_iter(it, loc);
            bool better = !best.value;
            if (!better) {
                if constexpr (Maximum) better = less(best.value, candidate, loc);
                else better = less(candidate, best.value, loc);
                rusk_guard_iter(it, loc);
            }
            if (better) {
                best.value = std::move(candidate);
                if constexpr (TrackPosition) best.index = index;
            }
        }
        rusk_guard_iter(it, loc);
    }
    rusk_guard_iter(it, loc);
    return best;
}
