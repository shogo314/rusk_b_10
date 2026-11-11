#include <type_traits>

template<class I> struct RuskEnumerated {
    Obj<I> source;
    std::int64_t index;
    bool ready = false;
    bool finished = false;
    bool overflow = false;
};

template<class I, class J> struct RuskZipped {
    Obj<I> left;
    Obj<J> right;
    bool ready = false;
    bool finished = false;
};

template<class T> struct RuskReverseVecIter {
    Obj<RuskVec<T>> owner;
    std::size_t next;
    std::uint64_t version;
};

struct RuskReverseStrIter {
    Obj<std::string> owner;
    std::size_t next, size;
};

struct RuskReverseRangeIter {
    std::int64_t next, start;
};

template<class I>
void rusk_guard_iter(const Obj<RuskEnumerated<I>>& it, Loc loc);
template<class I, class J>
void rusk_guard_iter(const Obj<RuskZipped<I, J>>& it, Loc loc);

template<class T> void rusk_guard_iter(const Obj<RuskReverseVecIter<T>>& it, Loc loc) {
    if (it->owner->version != it->version) fail(loc, "container changed during iteration");
}

inline void rusk_guard_iter(const Obj<RuskReverseStrIter>& it, Loc loc) {
    if (it->owner->size() != it->size) fail(loc, "string changed during iteration");
}

template<class I> void rusk_guard_iter(const Obj<RuskEnumerated<I>>& it, Loc loc) {
    if (!it->finished) rusk_guard_iter(it->source, loc);
}

template<class I, class J> void rusk_guard_iter(const Obj<RuskZipped<I, J>>& it, Loc loc) {
    if (!it->finished) {
        rusk_guard_iter(it->left, loc);
        rusk_guard_iter(it->right, loc);
    }
}

template<class I, class Has>
bool rusk_enumerate_ready(const Obj<RuskEnumerated<I>>& it, Loc loc, Has has) {
    if (it->finished) return false;
    rusk_guard_iter(it, loc);
    if (!it->ready) {
        bool ready = *has(it->source, loc);
        rusk_guard_iter(it, loc);
        it->ready = ready;
        it->finished = !ready;
    }
    return it->ready;
}

template<class I>
std::int64_t rusk_enumerate_index(const Obj<RuskEnumerated<I>>& it, Loc loc) {
    if (it->overflow) fail(loc, "enumerate index overflow");
    auto index = it->index;
    if (index == std::numeric_limits<std::int64_t>::max()) it->overflow = true;
    else ++it->index;
    it->ready = false;
    return index;
}

template<class I, class J>
Obj<RuskZipped<I, J>> rusk_zip_sources(Obj<I> left, Obj<J> right, Loc loc) {
    if constexpr (std::is_same_v<I, J>) {
        if (left.get() == right.get()) fail(loc, "zip requires distinct iterator states");
    }
    return make_obj<RuskZipped<I, J>>({std::move(left), std::move(right)});
}

template<class I, class J, class LeftHas, class RightHas>
bool rusk_zip_ready(const Obj<RuskZipped<I, J>>& it, Loc loc, LeftHas left_has, RightHas right_has) {
    if (it->finished) return false;
    rusk_guard_iter(it, loc);
    if (!it->ready) {
        bool left = *left_has(it->left, loc);
        rusk_guard_iter(it, loc);
        bool right = *right_has(it->right, loc);
        rusk_guard_iter(it, loc);
        if (left != right) fail(loc, "zip inputs have different lengths");
        it->ready = left;
        it->finished = !left;
    }
    return it->ready;
}

template<class T>
Obj<T> rusk_reverse_next(const Obj<RuskReverseVecIter<T>>& it, Loc loc) {
    rusk_guard_iter(it, loc);
    if (it->next == 0) fail(loc, "iterator exhausted");
    return (*it->owner)[--it->next];
}

inline Obj<char> rusk_reverse_next(const Obj<RuskReverseStrIter>& it, Loc loc) {
    rusk_guard_iter(it, loc);
    if (it->next == 0) fail(loc, "iterator exhausted");
    return string_get(it->owner, checked_int(static_cast<__int128>(--it->next), loc), loc);
}

inline std::int64_t rusk_reverse_next(const Obj<RuskReverseRangeIter>& it, Loc loc) {
    if (it->next <= it->start) fail(loc, "iterator exhausted");
    return --it->next;
}
