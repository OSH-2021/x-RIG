/*
 * Copyright 2020, Data61, CSIRO (ABN 41 687 119 230)
 *
 * SPDX-License-Identifier: GPL-2.0-only
 */

#include <types.h>
#include <api/failures.h>
#include <object/structures.h>

/* functions to manage the circular buffer of
 * sporadic budget replenishments (refills for short).
 *
 * The circular buffer always has at least one item in it.
 *
 * Items are appended at the tail (the back) and
 * removed from the head (the front). Below is
 * an example of a queue with 4 items (h = head, t = tail, x = item, [] = slot)
 * and max size 8.
 *
 * [][h][x][x][t][][][]
 *
 * and another example of a queue with 5 items
 *
 * [x][t][][][][h][x][x]
 *
 * The queue has a minimum size of 1, so it is possible that h == t.
 *
 * The queue is implemented as head + tail rather than head + size as
 * we cannot use the mod operator on all architectures without accessing
 * the fpu or implementing divide.
 */

/*Z 获取refill队列指定元素后的元素索引 */
/* return the index of the next item in the refill queue */
static inline word_t refill_next(sched_context_t *sc, word_t index)
{
    return (index == sc->scRefillMax - 1u) ? (0) : index + 1u;
}

#ifdef CONFIG_PRINTING
/* for debugging *//*Z 打印refill队列指定元素 */
UNUSED static inline void print_index(sched_context_t *sc, word_t index)
{

    printf("index %lu, Amount: %llx, time %llx\n", index, REFILL_INDEX(sc, index).rAmount,
           REFILL_INDEX(sc, index).rTime);
}
/*Z 打印refill队列所有元素 */
UNUSED static inline void refill_print(sched_context_t *sc)
{
    printf("Head %lu tail %lu\n", sc->scRefillHead, sc->scRefillTail);
    word_t current = sc->scRefillHead;
    /* always print the head */
    print_index(sc, current);

    while (current != sc->scRefillTail) {
        current = refill_next(sc, current);
        print_index(sc, current);
    }

}
#endif /* CONFIG_PRINTING */
#ifdef CONFIG_DEBUG_BUILD
/* check a refill queue is ordered correctly */
/*Z 检查refill队列元素是按预算启用时间升序排列的 */
static UNUSED bool_t refill_ordered(sched_context_t *sc)
{
    word_t current = sc->scRefillHead;
    word_t next = refill_next(sc, sc->scRefillHead);

    while (current != sc->scRefillTail) {
        if (!(REFILL_INDEX(sc, current).rTime <= REFILL_INDEX(sc, next).rTime)) {
            refill_print(sc);
            return false;
        }
        current = next;
        next = refill_next(sc, current);
    }

    return true;
}
/*Z 定义变量_sum记录所有refill时间片的和，并检查refill队列元素是按可以开始充值的时间升序排列的 */
#define REFILL_SANITY_START(sc) ticks_t _sum = refill_sum(sc); assert(refill_ordered(sc));
/*Z 检查SC的充值总和与预算相等 */
#define REFILL_SANITY_CHECK(sc, budget) \
    do { \
        assert(refill_sum(sc) == budget); assert(refill_ordered(sc)); \
    } while (0)
/*Z 检查SC的充值总和与先前定义的_sum相等 */
#define REFILL_SANITY_END(sc) \
    do {\
        REFILL_SANITY_CHECK(sc, _sum);\
    } while (0)
#else
#define REFILL_SANITY_START(sc)
#define REFILL_SANITY_CHECK(sc, budget)
#define REFILL_SANITY_END(sc)
#endif /* CONFIG_DEBUG_BUILD */
/*Z 获取所有refill元素时间片的累计和 */
/* compute the sum of a refill queue */
static UNUSED ticks_t refill_sum(sched_context_t *sc)
{
    ticks_t sum = REFILL_HEAD(sc).rAmount;
    word_t current = sc->scRefillHead;

    while (current != sc->scRefillTail) {
        current = refill_next(sc, current);
        sum += REFILL_INDEX(sc, current).rAmount;
    }

    return sum;
}
/*Z 取出refill元素队列的第1个元素 */
/* pop head of refill queue */
static inline refill_t refill_pop_head(sched_context_t *sc)
{
    /* queues cannot be smaller than 1 */
    assert(!refill_single(sc));

    UNUSED word_t prev_size = refill_size(sc);/*Z 现refill队列元素个数 */
    refill_t refill = REFILL_HEAD(sc);
    sc->scRefillHead = refill_next(sc, sc->scRefillHead);

    /* sanity */
    assert(prev_size == (refill_size(sc) + 1));
    assert(sc->scRefillHead < sc->scRefillMax);
    return refill;
}
/*Z 在refill队列尾追加一个refill项 */
/* add item to tail of refill queue */
static inline void refill_add_tail(sched_context_t *sc, refill_t refill)
{
    /* cannot add beyond queue size */
    assert(refill_size(sc) < sc->scRefillMax);

    word_t new_tail = refill_next(sc, sc->scRefillTail);
    sc->scRefillTail = new_tail;
    REFILL_TAIL(sc) = refill;

    /* sanity */
    assert(new_tail < sc->scRefillMax);
}
/*Z 对RoundRobin调度算法，追加一个无时间片的refill项(大概是用于记录已消耗的时间片) */
static inline void maybe_add_empty_tail(sched_context_t *sc)
{
    if (isRoundRobin(sc)) {
        /* add an empty refill - we track the used up time here */
        refill_t empty_tail = { .rTime = NODE_STATE(ksCurTime)};
        refill_add_tail(sc, empty_tail);
        assert(refill_size(sc) == MIN_REFILLS);
    }
}

#ifdef ENABLE_SMP_SUPPORT
void refill_new(sched_context_t *sc, word_t max_refills, ticks_t budget, ticks_t period, word_t core)
#else
void refill_new(sched_context_t *sc, word_t max_refills, ticks_t budget, ticks_t period)
#endif
{
    sc->scPeriod = period;
    sc->scRefillHead = 0;
    sc->scRefillTail = 0;
    sc->scRefillMax = max_refills;
    assert(budget > MIN_BUDGET);
    /* full budget available */
    REFILL_HEAD(sc).rAmount = budget;
    /* budget can be used from now */
    REFILL_HEAD(sc).rTime = NODE_STATE_ON_CORE(ksCurTime, core);
    maybe_add_empty_tail(sc);
    REFILL_SANITY_CHECK(sc, budget);
}

void refill_update(sched_context_t *sc, ticks_t new_period, ticks_t new_budget, word_t new_max_refills)
{

    /* refill must be initialised in order to be updated - otherwise refill_new should be used */
    assert(sc->scRefillMax > 0);

    /* this is called on an active thread. We want to preserve the sliding window constraint -
     * so over new_period, new_budget should not be exceeded even temporarily */

    /* move the head refill to the start of the list - it's ok as we're going to truncate the
     * list to size 1 - and this way we can't be in an invalid list position once new_max_refills
     * is updated */
    REFILL_INDEX(sc, 0) = REFILL_HEAD(sc);
    sc->scRefillHead = 0;
    /* truncate refill list to size 1 */
    sc->scRefillTail = sc->scRefillHead;
    /* update max refills */
    sc->scRefillMax = new_max_refills;
    /* update period */
    sc->scPeriod = new_period;

    if (refill_ready(sc)) {
        REFILL_HEAD(sc).rTime = NODE_STATE_ON_CORE(ksCurTime, sc->scCore);
    }

    if (REFILL_HEAD(sc).rAmount >= new_budget) {
        /* if the heads budget exceeds the new budget just trim it */
        REFILL_HEAD(sc).rAmount = new_budget;
        maybe_add_empty_tail(sc);
    } else {
        /* otherwise schedule the rest for the next period */
        refill_t new = { .rAmount = (new_budget - REFILL_HEAD(sc).rAmount),
                         .rTime = REFILL_HEAD(sc).rTime + new_period
                       };
        refill_add_tail(sc, new);
    }

    REFILL_SANITY_CHECK(sc, new_budget);
}
/*Z 将已消费的预算回填到尾元素(或追加)。不好：隐含了refill元素队列不能满的要求 */
static inline void schedule_used(sched_context_t *sc, refill_t new)
{
    /* schedule the used amount */
    if (new.rAmount < MIN_BUDGET && !refill_single(sc)) {   /*Z 如果预算过小且元素个数不少于2个 */
        /* used amount is to small - merge with last and delay */
        REFILL_TAIL(sc).rAmount += new.rAmount;                 /*Z 则加到尾，视情推迟启用 */
        REFILL_TAIL(sc).rTime = MAX(new.rTime, REFILL_TAIL(sc).rTime);
    } else if (new.rTime <= REFILL_TAIL(sc).rTime) {        /*Z 否则如果启用时间比尾元素的早 */
        REFILL_TAIL(sc).rAmount += new.rAmount;                 /*Z 则加到尾 */
    } else {                                                /*Z 否则 */
        refill_add_tail(sc, new);                               /*Z 追加尾元素 */
    }
}
/*Z 剩余不足(refill队列满也认为是)调用。根据参数，执行预算，更新refill队列。逻辑比较乱，没看懂??????????? */
void refill_budget_check(ticks_t usage,     /*Z 已用 */
                         ticks_t capacity)  /*Z 剩余。它俩加一起是头refill单元预算 */
{
    sched_context_t *sc = NODE_STATE(ksCurSC);
    /* this function should only be called when the sc is out of budget *//*Z 或refill队列满 */
    assert(capacity < MIN_BUDGET || refill_full(sc));
    assert(sc->scPeriod > 0);
    REFILL_SANITY_START(sc);
    /*Z 一点没剩，可能还欠 */
    if (capacity == 0) {
        while (REFILL_HEAD(sc).rAmount <= usage) {/*Z 充值单元依次抵用消费直至足够抵用，不管是不是达到启用时间 */
            /* exhaust and schedule replenishment */
            usage -= REFILL_HEAD(sc).rAmount;/*Z 用头元素的全部预算抵减已用 */
            if (refill_single(sc)) {
                /* update in place */
                REFILL_HEAD(sc).rTime += sc->scPeriod;
            } else {
                refill_t old_head = refill_pop_head(sc);
                old_head.rTime = old_head.rTime + sc->scPeriod;
                schedule_used(sc, old_head);/*Z 将已消费的预算回填到尾元素(或追加) */
            }
        }

        /* budget overrun */
        if (usage > 0) {
            /* budget reduced when calculating capacity */
            /* due to overrun delay next replenishment */
            REFILL_HEAD(sc).rTime += usage;/*Z ????? 惩罚 */
            /* merge front two replenishments if times overlap */
            if (!refill_single(sc) &&
                REFILL_HEAD(sc).rTime + REFILL_HEAD(sc).rAmount >=
                REFILL_INDEX(sc, refill_next(sc, sc->scRefillHead)).rTime) {

                refill_t refill = refill_pop_head(sc);
                REFILL_HEAD(sc).rAmount += refill.rAmount;
                REFILL_HEAD(sc).rTime = refill.rTime;
            }
        }
    }

    capacity = refill_capacity(sc, usage);/*Z 扣除已用后可用 */
    if (capacity > 0 && refill_ready(sc)) {/*Z 仍有可用且可以使用 */
        refill_split_check(usage);/*Z 执行预算，更新refill队列 */
    }
    /*Z 调整refill队列合规：预算不低于最小值、队列有空余 */
    /* ensure the refill head is sufficient, such that when we wake in awaken,
     * there is enough budget to run */
    while (REFILL_HEAD(sc).rAmount < MIN_BUDGET || refill_full(sc)) {
        refill_t refill = refill_pop_head(sc);
        REFILL_HEAD(sc).rAmount += refill.rAmount;
        /* this loop is guaranteed to terminate as the sum of
         * rAmount in a refill must be >= MIN_BUDGET */
    }

    REFILL_SANITY_END(sc);
}
/*Z 对当前调度上下文，按照“循环预算、减头加尾、总量不减”的原则，用已消费时间扣除头元素预算，更新refill元素队列：
循环预算：扣除的预算循环追加到队列尾
减头加尾：已消费的从头中扣并转入尾，剩余的加给下一个
总量不减：各元素预算时间总和不变 */
void refill_split_check(ticks_t usage)
{
    sched_context_t *sc = NODE_STATE(ksCurSC);
    /* invalid to call this on a NULL sc */
    assert(sc != NULL);
    /* something is seriously wrong if this is called and no
     * time has been used */
    assert(usage > 0);
    assert(usage <= REFILL_HEAD(sc).rAmount);
    assert(sc->scPeriod > 0);

    REFILL_SANITY_START(sc);
    /*Z 用头元素计算剩余预算 */
    /* first deal with the remaining budget of the current replenishment */
    ticks_t remnant = REFILL_HEAD(sc).rAmount - usage;
    /*Z 新建一个临时的refill元素：下一个周期的预算 */
    /* set up a new replenishment structure */
    refill_t new = (refill_t) {
        .rAmount = usage, .rTime = REFILL_HEAD(sc).rTime + sc->scPeriod
    };

    if (refill_size(sc) == sc->scRefillMax || remnant < MIN_BUDGET) {
        /* merge remnant with next replenishment - either it's too small
         * or we're out of space */
        if (refill_single(sc)) {/*Z 1. 只有一个元素且剩余预算过小，丢弃剩余(执行时间小于预算时间)，更新元素 */
            /* update inplace */
            new.rAmount += remnant;
            REFILL_HEAD(sc) = new;
        } else {                /*Z 2. 元素队列满或剩余预算过小(多个元素)，剔除头，剩余的加给下一个，已消费的追加到尾 */
            refill_pop_head(sc);
            REFILL_HEAD(sc).rAmount += remnant;
            schedule_used(sc, new);
        }
        assert(refill_ordered(sc));
    } else {                    /*Z 3. 剩余的够大且元素队列不満，减头预算，已消费的追加到尾 */
        /* leave remnant as reduced replenishment */
        assert(remnant >= MIN_BUDGET);
        /* split the head refill  */
        REFILL_HEAD(sc).rAmount = remnant;
        schedule_used(sc, new);
    }

    REFILL_SANITY_END(sc);
}
/*Z 检查指定上下文的refill队列，对可以开始充值、且“充值开始时间+预算值”与“下个充值开始时间”有重叠的，进行合并 */
void refill_unblock_check(sched_context_t *sc)
{
    /*Z 轮转调度无充值一说 */
    if (isRoundRobin(sc)) {
        /* nothing to do */
        return;
    }

    /* advance earliest activation time to now */
    REFILL_SANITY_START(sc);
    if (refill_ready(sc)) {
        REFILL_HEAD(sc).rTime = NODE_STATE_ON_CORE(ksCurTime, sc->scCore);
        NODE_STATE(ksReprogram) = true;

        /* merge available replenishments */
        while (!refill_single(sc)) {
            ticks_t amount = REFILL_HEAD(sc).rAmount;
            if (REFILL_INDEX(sc, refill_next(sc, sc->scRefillHead)).rTime <= NODE_STATE_ON_CORE(ksCurTime, sc->scCore) + amount) {
                refill_pop_head(sc);
                REFILL_HEAD(sc).rAmount += amount;
                REFILL_HEAD(sc).rTime = NODE_STATE_ON_CORE(ksCurTime, sc->scCore);
            } else {
                break;
            }
        }

        assert(refill_sufficient(sc, 0));/*Z 可充的TICKs是否足够用于进出内核 */
    }
    REFILL_SANITY_END(sc);
}
