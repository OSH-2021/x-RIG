/*
 * Copyright 2020, Data61, CSIRO (ABN 41 687 119 230)
 *
 * SPDX-License-Identifier: GPL-2.0-only
 */

#include <machine/timer.h>
#include <mode/api/ipc_buffer.h>
#include <object/schedcontext.h>
#include <object/schedcontrol.h>
#include <kernel/sporadic.h>

static exception_t invokeSchedControl_Configure(sched_context_t *target, word_t core, ticks_t budget,
                                                ticks_t period, word_t max_refills, word_t badge)
{/*Z 参数依次为：目标SC、所在CPU、预算(tick)、周期(tick)、最大refill数量、标记 */

    target->scBadge = badge;
    /*Z 确保SC的线程暂停、不在ready/release队列、已消费时间记账 */
    /* don't modify parameters of tcb while it is in a sorted queue */
    if (target->scTcb) {
        /* possibly stall a remote core */
        SMP_COND_STATEMENT(remoteTCBStall(target->scTcb));
        /* remove from scheduler */
        tcbReleaseRemove(target->scTcb);
        tcbSchedDequeue(target->scTcb);
        /* bill the current consumed amount before adjusting the params */
        if (NODE_STATE_ON_CORE(ksCurSC, target->scCore) == target) {/*Z SC处于执行态 */
#ifdef ENABLE_SMP_SUPPORT
            if (target->scCore == getCurrentCPUIndex()) {/*Z 如在当前CPU */
#endif /* ENABLE_SMP_SUPPORT */
                if (checkBudget()) {
                    commitTime();
                }
#ifdef ENABLE_SMP_SUPPORT
            } else {/*Z 如在其它CPU */
                /* if its a remote core, manually charge the budget */
                ticks_t capacity = refill_capacity(target, NODE_STATE_ON_CORE(ksConsumed, target->scCore));

                chargeBudget(capacity, NODE_STATE_ON_CORE(ksConsumed, target->scCore), false, target->scCore, false);
                doReschedule(target->scCore);
            }
#endif /* ENABLE_SMP_SUPPORT */
        }
    }

    if (budget == period) {
        /* this is a cool hack: for round robin, we set the
         * period to 0, which means that the budget will always be ready to be refilled
         * and avoids some special casing.
         */
        period = 0;
        max_refills = MIN_REFILLS;
    }

    if (SMP_COND_STATEMENT(core == target->scCore &&) target->scRefillMax > 0 && target->scTcb
        && isRunnable(target->scTcb)) {
        /* the scheduling context is active - it can be used, so
         * we need to preserve the bandwidth */
        refill_update(target, period, budget, max_refills);
    } else {
        /* the scheduling context isn't active - it's budget is not being used, so
         * we can just populate the parameters from now */
        REFILL_NEW(target, max_refills, budget, period, core);
    }

#ifdef ENABLE_SMP_SUPPORT
    target->scCore = core;
    if (target->scTcb) {
        migrateTCB(target->scTcb, target->scCore);
    }
#endif /* ENABLE_SMP_SUPPORT */

    if (target->scTcb && target->scRefillMax > 0) {
        schedContext_resume(target);/*Z 如果线程可调度但预算不可用或不足，则将线程加入release队列 */
        if (SMP_TERNARY(core == CURRENT_CPU_INDEX(), true)) {
            if (isRunnable(target->scTcb) && target->scTcb != NODE_STATE(ksCurThread)) {
                possibleSwitchTo(target->scTcb);
            }
        } else if (isRunnable(target->scTcb)) {
            SCHED_ENQUEUE(target->scTcb);
        }
        if (target->scTcb == NODE_STATE(ksCurThread)) {
            rescheduleRequired();
        }
    }

    return EXCEPTION_NONE;
}
/*Z 引用cap_sched_control_cap配置目标SC */
static exception_t decodeSchedControl_Configure(word_t length, cap_t cap, extra_caps_t extraCaps, word_t *buffer)
{
    if (extraCaps.excaprefs[0] == NULL) {
        userError("SchedControl_Configure: Truncated message.");
        current_syscall_error.type = seL4_TruncatedMessage;
        return EXCEPTION_SYSCALL_ERROR;
    }

    if (length < (TIME_ARG_SIZE * 2) + 2) {
        userError("SchedControl_configure: truncated message.");
        current_syscall_error.type = seL4_TruncatedMessage;
        return EXCEPTION_SYSCALL_ERROR;
    }

    time_t budget_us = mode_parseTimeArg(0, buffer);/*Z --------------------------------------消息传参：0-每周期最大执行时间(微秒)：预算：b */
    time_t period_us = mode_parseTimeArg(TIME_ARG_SIZE, buffer);                                    /*Z 1-周期(微秒)：p */
    word_t extra_refills = getSyscallArg(TIME_ARG_SIZE * 2, buffer);                                /*Z 2-额外refill单元数量 */
    word_t badge = getSyscallArg(TIME_ARG_SIZE * 2 + 1, buffer);                                    /*Z 3-SC标记 */

    cap_t targetCap = extraCaps.excaprefs[0]->cap;                                                  /*Z extraCaps0-目标SC */
    if (unlikely(cap_get_capType(targetCap) != cap_sched_context_cap)) {
        userError("SchedControl_Configure: target cap not a scheduling context cap");
        current_syscall_error.type = seL4_InvalidCapability;
        current_syscall_error.invalidCapNumber = 1;
        return EXCEPTION_SYSCALL_ERROR;
    }

    if (budget_us > getMaxUsToTicks() || budget_us < MIN_BUDGET_US) {
        userError("SchedControl_Configure: budget out of range.");
        current_syscall_error.type = seL4_RangeError;
        current_syscall_error.rangeErrorMin = MIN_BUDGET_US;
        current_syscall_error.rangeErrorMax = getMaxUsToTicks();
        return EXCEPTION_SYSCALL_ERROR;
    }

    if (period_us > getMaxUsToTicks() || period_us < MIN_BUDGET_US) {
        userError("SchedControl_Configure: period out of range.");
        current_syscall_error.type = seL4_RangeError;
        current_syscall_error.rangeErrorMin = MIN_BUDGET_US;
        current_syscall_error.rangeErrorMax = getMaxUsToTicks();
        return EXCEPTION_SYSCALL_ERROR;
    }

    if (budget_us > period_us) {
        userError("SchedControl_Configure: budget must be <= period");
        current_syscall_error.type = seL4_RangeError;
        current_syscall_error.rangeErrorMin = MIN_BUDGET_US;
        current_syscall_error.rangeErrorMax = period_us;
        return EXCEPTION_SYSCALL_ERROR;
    }

    if (extra_refills + MIN_REFILLS > refill_absolute_max(targetCap)) {
        current_syscall_error.type = seL4_RangeError;
        current_syscall_error.rangeErrorMin = 0;
        current_syscall_error.rangeErrorMax = refill_absolute_max(targetCap) - MIN_REFILLS;
        userError("Max refills invalid, got %lu, max %lu",
                  extra_refills,
                  current_syscall_error.rangeErrorMax);
        return EXCEPTION_SYSCALL_ERROR;
    }

    setThreadState(NODE_STATE(ksCurThread), ThreadState_Restart);
    return invokeSchedControl_Configure(SC_PTR(cap_sched_context_cap_get_capSCPtr(targetCap)),
                                        cap_sched_control_cap_get_core(cap),
                                        usToTicks(budget_us),
                                        usToTicks(period_us),
                                        extra_refills + MIN_REFILLS,
                                        badge);
}
/*Z cap_sched_control_cap的Call系统调用： */
exception_t decodeSchedControlInvocation(word_t label, cap_t cap, word_t length, extra_caps_t extraCaps,
                                         word_t *buffer)
{
    switch (label) {
    case SchedControlConfigure:
        return  decodeSchedControl_Configure(length, cap, extraCaps, buffer);
    default:
        userError("SchedControl invocation: Illegal operation attempted.");
        current_syscall_error.type = seL4_IllegalOperation;
        return EXCEPTION_SYSCALL_ERROR;
    }
}
