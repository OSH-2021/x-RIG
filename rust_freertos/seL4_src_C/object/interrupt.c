/*
 * Copyright 2014, General Dynamics C4 Systems
 *
 * SPDX-License-Identifier: GPL-2.0-only
 */

#include <assert.h>
#include <types.h>
#include <api/failures.h>
#include <api/invocation.h>
#include <api/syscall.h>
#include <machine/io.h>
#include <object/structures.h>
#include <object/interrupt.h>
#include <object/cnode.h>
#include <object/notification.h>
#include <kernel/cspace.h>
#include <kernel/thread.h>
#include <model/statedata.h>
#include <machine/timer.h>
#include <smp/ipi.h>
/*Z 引用cap_irq_control_cap能力的系统调用：创建中断处理能力 */
exception_t decodeIRQControlInvocation(word_t invLabel, word_t length,/*Z 消息标签、长度 */
                                       cte_t *srcSlot, extra_caps_t excaps,/*Z 引用的CSlot、额外CSlot */
                                       word_t *buffer)/*Z IPC buffer */
{
    if (invLabel == IRQIssueIRQHandler) {/*Z ---------------------------------------------子功能：新建中断处理能力，仅适用于老旧的PIC。应该是个过时的功能 */
        word_t index, depth, irq_w;
        irq_t irq;
        cte_t *destSlot;
        cap_t cnodeCap;
        lookupSlot_ret_t lu_ret;
        exception_t status;

        if (length < 3 || excaps.excaprefs[0] == NULL) {
            current_syscall_error.type = seL4_TruncatedMessage;
            return EXCEPTION_SYSCALL_ERROR;
        }
        irq_w = getSyscallArg(0, buffer);/*Z -----------------------------------------消息传参：0-要处理的IRQ */
        irq = CORE_IRQ_TO_IRQT(0, irq_w);
        index = getSyscallArg(1, buffer);                                                   /*Z 1-要存放处理能力的句柄 */
        depth = getSyscallArg(2, buffer);                                                   /*Z 2-深度 */

        cnodeCap = excaps.excaprefs[0]->cap;                                                /*Z extraCaps0-所属的CNode */

        status = Arch_checkIRQ(irq_w);
        if (status != EXCEPTION_NONE) {
            return status;
        }
        /*Z IRQ是否已启用 */
        if (isIRQActive(irq)) {
            current_syscall_error.type = seL4_RevokeFirst;
            userError("Rejecting request for IRQ %u. Already active.", (int)IRQT_TO_IRQ(irq));
            return EXCEPTION_SYSCALL_ERROR;
        }

        lu_ret = lookupTargetSlot(cnodeCap, index, depth);
        if (lu_ret.status != EXCEPTION_NONE) {
            userError("Target slot for new IRQ Handler cap invalid: cap %lu, IRQ %u.",
                      getExtraCPtr(buffer, 0), (int)IRQT_TO_IRQ(irq));
            return lu_ret.status;
        }
        destSlot = lu_ret.slot;

        status = ensureEmptySlot(destSlot);
        if (status != EXCEPTION_NONE) {
            userError("Target slot for new IRQ Handler cap not empty: cap %lu, IRQ %u.",
                      getExtraCPtr(buffer, 0), (int)IRQT_TO_IRQ(irq));
            return status;
        }

        setThreadState(NODE_STATE(ksCurThread), ThreadState_Restart);
        return invokeIRQControl(irq, destSlot, srcSlot);/*Z 启用该irq，为其建立中断处理能力 */
    } else {/*Z 创建中断处理能力 */
        return Arch_decodeIRQControlInvocation(invLabel, length, srcSlot, excaps, buffer);
    }
}
/*Z 启用该irq，为其建立中断处理能力 */
exception_t invokeIRQControl(irq_t irq, cte_t *handlerSlot, cte_t *controlSlot)
{   /*Z 设置第irq个硬件中断的基本属性和状态全局变量 */
    setIRQState(IRQSignal, irq);
    cteInsert(cap_irq_handler_cap_new(IRQT_TO_IDX(irq)), controlSlot, handlerSlot);

    return EXCEPTION_NONE;
}
/*Z 引用cap_irq_handler_cap能力的系统调用 */
exception_t decodeIRQHandlerInvocation(word_t invLabel, irq_t irq,/*Z 消息标签、IRQ号 */
                                       extra_caps_t excaps)
{
    switch (invLabel) {
    case IRQAckIRQ:/*Z -------------------------------------------------------------子功能：启用该IRQ。无消息传参 */
        setThreadState(NODE_STATE(ksCurThread), ThreadState_Restart);
        invokeIRQHandler_AckIRQ(irq);/*Z 启用该irq硬件中断 */
        return EXCEPTION_NONE;

    case IRQSetIRQHandler: {/*Z ----------------------------------------------------子功能：设置中断处理函数 */
        cap_t ntfnCap;
        cte_t *slot;

        if (excaps.excaprefs[0] == NULL) {
            current_syscall_error.type = seL4_TruncatedMessage;
            return EXCEPTION_SYSCALL_ERROR;
        }
        ntfnCap = excaps.excaprefs[0]->cap;/*Z -------------------------------------消息传参：extraCaps0-绑定的通知能力 */
        slot = excaps.excaprefs[0];

        if (cap_get_capType(ntfnCap) != cap_notification_cap ||
            !cap_notification_cap_get_capNtfnCanSend(ntfnCap)) {/*Z 内核引用能发送中断通知，处理函数引用能发送处理完毕通知 */
            if (cap_get_capType(ntfnCap) != cap_notification_cap) {
                userError("IRQSetHandler: provided cap is not an notification capability.");
            } else {
                userError("IRQSetHandler: caller does not have send rights on the endpoint.");
            }
            current_syscall_error.type = seL4_InvalidCapability;
            current_syscall_error.invalidCapNumber = 0;
            return EXCEPTION_SYSCALL_ERROR;
        }

        setThreadState(NODE_STATE(ksCurThread), ThreadState_Restart);
        invokeIRQHandler_SetIRQHandler(irq, ntfnCap, slot);/*Z 在全局IRQ CNode中建立该irq通知能力的拷贝，并建立CSlot链接 */
        return EXCEPTION_NONE;
    }

    case IRQClearIRQHandler:/*Z ----------------------------------------------------子功能：清除中断处理函数 */
        setThreadState(NODE_STATE(ksCurThread), ThreadState_Restart);
        invokeIRQHandler_ClearIRQHandler(irq);/*Z 在全局IRQ CNode中该irq的能力 */
        return EXCEPTION_NONE;

    default:
        userError("IRQHandler: Illegal operation.");
        current_syscall_error.type = seL4_IllegalOperation;
        return EXCEPTION_SYSCALL_ERROR;
    }
}
/*Z 启用该irq硬件中断 */
void invokeIRQHandler_AckIRQ(irq_t irq)
{
#ifdef CONFIG_ARCH_RISCV
    plic_complete_claim(irq);
#else
#if defined ENABLE_SMP_SUPPORT && defined CONFIG_ARCH_ARM
    if (IRQ_IS_PPI(irq) && IRQT_TO_CORE(irq) != getCurrentCPUIndex()) {
        doRemoteMaskPrivateInterrupt(IRQT_TO_CORE(irq), false, IRQT_TO_IDX(irq));
        return;
    }
#endif
    maskInterrupt(false, irq);
#endif
}
/*Z 在全局IRQ CNode中建立该irq通知能力的拷贝，并建立CSlot链接 */
void invokeIRQHandler_SetIRQHandler(irq_t irq, cap_t cap, cte_t *slot)
{
    cte_t *irqSlot;

    irqSlot = intStateIRQNode + IRQT_TO_IDX(irq);
    /** GHOSTUPD: "(True, gs_set_assn cteDeleteOne_'proc (-1))" */
    cteDeleteOne(irqSlot);/*Z 删除指定的CSlot */
    cteInsert(cap, slot, irqSlot);/*Z 源、目的CSlot建立关联，并将新能力拷贝到目的CSlot */
}
/*Z 在全局IRQ CNode中该irq的能力 */
void invokeIRQHandler_ClearIRQHandler(irq_t irq)
{
    cte_t *irqSlot;

    irqSlot = intStateIRQNode + IRQT_TO_IDX(irq);
    /** GHOSTUPD: "(True, gs_set_assn cteDeleteOne_'proc (-1))" */
    cteDeleteOne(irqSlot);/*Z 删除指定的CSlot */
}
/*Z 解除中断处理线程与IRQ的关联，置空IRQ CNode中的相应能力 */
void deletingIRQHandler(irq_t irq)
{
    cte_t *slot;

    slot = intStateIRQNode + IRQT_TO_IDX(irq);
    /** GHOSTUPD: "(True, gs_set_assn cteDeleteOne_'proc (ucast cap_notification_cap))" */
    cteDeleteOne(slot);
}
/*Z 设置第irq个硬件中断的基本属性和状态全局变量为禁用 */
void deletedIRQHandler(irq_t irq)
{
    setIRQState(IRQInactive, irq);
}
/*Z 处理32~159硬件中断，普通中断发给用户自定义处理线程，定时器递减时间片，其它特定处理 */
void handleInterrupt(irq_t irq)
{   /*Z 对超范围的IRQ，屏蔽并发送中断服务完成信号 */
    if (unlikely(IRQT_TO_IRQ(irq) > maxIRQ)) {
        /* mask, ack and pretend it didn't happen. We assume that because
         * the interrupt controller for the platform returned this IRQ that
         * it is safe to use in mask and ack operations, even though it is
         * above the claimed maxIRQ. i.e. we're assuming maxIRQ is wrong */
        printf("Received IRQ %d, which is above the platforms maxIRQ of %d\n", (int)IRQT_TO_IRQ(irq), (int)maxIRQ);
        maskInterrupt(true, irq);
        ackInterrupt(irq);
        return;
    }
    switch (intStateIRQTable[IRQT_TO_IDX(irq)]) {/*Z 硬件IRQ基本属性 */
    case IRQSignal: {/*Z 未禁用普通IRQ */
        cap_t cap;
        /*Z 获取其能力 */
        cap = intStateIRQNode[IRQT_TO_IDX(irq)].cap;
        /*Z 向能力所指中断处理线程发送处理通知 */
        if (cap_get_capType(cap) == cap_notification_cap &&
            cap_notification_cap_get_capNtfnCanSend(cap)) {
            sendSignal(NTFN_PTR(cap_notification_cap_get_capNtfnPtr(cap)),
                       cap_notification_cap_get_capNtfnBadge(cap));
        } else {
#ifdef CONFIG_IRQ_REPORTING
            printf("Undelivered IRQ: %d\n", (int)IRQT_TO_IRQ(irq));
#endif
        }
#ifndef CONFIG_ARCH_RISCV
        maskInterrupt(true, irq);/*Z 临时禁用此同一中断，因为正在处理中 */
#endif
        break;
    }

    case IRQTimer:/*Z 定时器中断 */
#ifdef CONFIG_KERNEL_MCS
        ackDeadlineIRQ();
        NODE_STATE(ksReprogram) = true;
#else   /*Z 递减当前线程和调度域时间片，达到时限时设置重调度 */
        timerTick();
        resetTimer();
#endif
        break;

#ifdef ENABLE_SMP_SUPPORT
    case IRQIPI:
        handleIPI(irq, true);
        break;
#endif /* ENABLE_SMP_SUPPORT */

    case IRQReserved:
        handleReservedIRQ(irq);
        break;

    case IRQInactive:
        /*
         * This case shouldn't happen anyway unless the hardware or
         * platform code is broken. Hopefully masking it again should make
         * the interrupt go away.
         */
        maskInterrupt(true, irq);
#ifdef CONFIG_IRQ_REPORTING
        printf("Received disabled IRQ: %d\n", (int)IRQT_TO_IRQ(irq));
#endif
        break;

    default:
        /* No corresponding haskell error */
        fail("Invalid IRQ state");
    }

    ackInterrupt(irq);
}
/*Z IRQ是否已启用 */
bool_t isIRQActive(irq_t irq)
{
    return intStateIRQTable[IRQT_TO_IDX(irq)] != IRQInactive;
}
/*Z 设置第irq个硬件中断的基本属性和状态全局变量（包括是否禁用）*/
void setIRQState(irq_state_t irqState, irq_t irq)
{
    intStateIRQTable[IRQT_TO_IDX(irq)] = irqState;
#if defined ENABLE_SMP_SUPPORT && defined CONFIG_ARCH_ARM
    if (IRQ_IS_PPI(irq) && IRQT_TO_CORE(irq) != getCurrentCPUIndex()) {
        doRemoteMaskPrivateInterrupt(IRQT_TO_CORE(irq), irqState == IRQInactive, IRQT_TO_IDX(irq));
        return;
    }
#endif
    maskInterrupt(irqState == IRQInactive, irq);/*Z 设置第irq个硬件中断的禁用掩码 */
}
