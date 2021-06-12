/*
 * Copyright 2014, General Dynamics C4 Systems
 *
 * SPDX-License-Identifier: GPL-2.0-only
 */

#include <api/failures.h>
#include <kernel/cspace.h>
#include <kernel/faulthandler.h>
#include <kernel/thread.h>
#include <machine/io.h>
#include <arch/machine.h>

#ifdef CONFIG_KERNEL_MCS
void handleFault(tcb_t *tptr)
{
    bool_t hasFaultHandler = sendFaultIPC(tptr, TCB_PTR_CTE_PTR(tptr, tcbFaultHandler)->cap,
                                          tptr->tcbSchedContext != NULL);
    if (!hasFaultHandler) {
        handleNoFaultHandler(tptr);
    }
}
/*Z 将超时错误发给线程超时错误处理句柄 */
void handleTimeout(tcb_t *tptr)
{
    assert(validTimeoutHandler(tptr));
    sendFaultIPC(tptr, TCB_PTR_CTE_PTR(tptr, tcbTimeoutHandler)->cap, false);
}
/*Z 将内核错误以指定线程(即导致错误的当前线程)为发送方，以指定能力为接收方，按阻塞方式发送IPC */
bool_t sendFaultIPC(tcb_t *tptr, cap_t handlerCap, bool_t can_donate)
{
    if (cap_get_capType(handlerCap) == cap_endpoint_cap) {
        assert(cap_endpoint_cap_get_capCanSend(handlerCap));
        assert(cap_endpoint_cap_get_capCanGrant(handlerCap) ||
               cap_endpoint_cap_get_capCanGrantReply(handlerCap));

        tptr->tcbFault = current_fault;
        sendIPC(true, false,
                cap_endpoint_cap_get_capEPBadge(handlerCap),
                cap_endpoint_cap_get_capCanGrant(handlerCap),
                cap_endpoint_cap_get_capCanGrantReply(handlerCap),
                can_donate, tptr,
                EP_PTR(cap_endpoint_cap_get_capEPPtr(handlerCap)));

        return true;
    } else {
        assert(cap_get_capType(handlerCap) == cap_null_cap);
        return false;
    }
}
#else
/*Z 将内核错误发送给线程(均为当前线程)的错误处理对象，线程阻塞并等待处理结果 */
void handleFault(tcb_t *tptr)
{
    exception_t status;
    seL4_Fault_t fault = current_fault;
    /*Z 将内核错误以指定线程(即导致错误的当前线程)为发送方，以其错误处理线程为接收方，按阻塞并等待处理结果方式发送IPC */
    status = sendFaultIPC(tptr);
    if (status != EXCEPTION_NONE) {
        handleDoubleFault(tptr, fault);/*Z 打印二次错误信息并置线程为不活跃状态。参数为第一次错误，第二次错误在全局变量中 */
    }
}
/*Z 将内核错误以指定线程(即导致错误的当前线程)为发送方，以其错误处理线程为接收方，按阻塞并等待处理结果方式发送IPC */
exception_t sendFaultIPC(tcb_t *tptr)
{
    cptr_t handlerCPtr;
    cap_t  handlerCap;
    lookupCap_ret_t lu_ret;
    lookup_fault_t original_lookup_fault;
    /*Z 记录原来的查询错误 */
    original_lookup_fault = current_lookup_fault;
    /*Z 查找线程的错误处理CSlot句柄指示的能力 */
    handlerCPtr = tptr->tcbFaultHandler;
    lu_ret = lookupCap(tptr, handlerCPtr);
    if (lu_ret.status != EXCEPTION_NONE) {
        current_fault = seL4_Fault_CapFault_new(handlerCPtr, false);
        return EXCEPTION_FAULT;
    }
    handlerCap = lu_ret.cap;
                                                /*Z 内核给线程发消息：*/
    if (cap_get_capType(handlerCap) == cap_endpoint_cap &&      /*Z 线程有端点能力 */
        cap_endpoint_cap_get_capCanSend(handlerCap) &&          /*Z     有能发送能力(线程代表内核向错误处理线程发) */
        (cap_endpoint_cap_get_capCanGrant(handlerCap) ||        /*Z     有能授权或能授权回复能力(以保证接收处理结果) */
         cap_endpoint_cap_get_capCanGrantReply(handlerCap))) {
        tptr->tcbFault = current_fault;                                             /*Z 取得前期设置的全局错误值 */
        if (seL4_Fault_get_seL4_FaultType(current_fault) == seL4_Fault_CapFault) {  /*Z 如果是能力错误方面的消息，带上原始查询错误 */
            tptr->tcbLookupFailure = original_lookup_fault;
        }
        /*Z 阻塞并等待处理结果方式，将错误消息copy给错误处理线程 */
        sendIPC(true/*Z 阻塞 */, true/*Z 要回复 */,
                cap_endpoint_cap_get_capEPBadge(handlerCap),/*Z 端点标记 */
                cap_endpoint_cap_get_capCanGrant(handlerCap), true/*Z 能授权回复，以保证接收处理结果 */, tptr,
                EP_PTR(cap_endpoint_cap_get_capEPPtr(handlerCap)));

        return EXCEPTION_NONE;
    } else {/*Z 缺少应有的能力 */
        current_fault = seL4_Fault_CapFault_new(handlerCPtr, false);
        current_lookup_fault = lookup_fault_missing_capability_new(0);

        return EXCEPTION_FAULT;
    }
}
#endif

#ifdef CONFIG_PRINTING
/*Z 打印seL4_Fault_t错误信息 */
static void print_fault(seL4_Fault_t f)
{
    switch (seL4_Fault_get_seL4_FaultType(f)) {
    case seL4_Fault_NullFault:
        printf("null fault");
        break;
    case seL4_Fault_CapFault:
        printf("cap fault in %s phase at address %p",
               seL4_Fault_CapFault_get_inReceivePhase(f) ? "receive" : "send",
               (void *)seL4_Fault_CapFault_get_address(f));
        break;
    case seL4_Fault_VMFault:
        printf("vm fault on %s at address %p with status %p",
               seL4_Fault_VMFault_get_instructionFault(f) ? "code" : "data",
               (void *)seL4_Fault_VMFault_get_address(f),
               (void *)seL4_Fault_VMFault_get_FSR(f));
        break;
    case seL4_Fault_UnknownSyscall:
        printf("unknown syscall %p",
               (void *)seL4_Fault_UnknownSyscall_get_syscallNumber(f));
        break;
    case seL4_Fault_UserException:
        printf("user exception %p code %p",
               (void *)seL4_Fault_UserException_get_number(f),
               (void *)seL4_Fault_UserException_get_code(f));
        break;
#ifdef CONFIG_KERNEL_MCS
    case seL4_Fault_Timeout:
        printf("Timeout fault for 0x%x\n", (unsigned int) seL4_Fault_Timeout_get_badge(f));
        break;
#endif
    default:
        printf("unknown fault");
        break;
    }
}
#endif

#ifdef CONFIG_KERNEL_MCS
void handleNoFaultHandler(tcb_t *tptr)
#else
/* The second fault, ex2, is stored in the global current_fault */
/*Z 打印二次错误信息并置线程为不活跃状态。参数为第一次错误，第二次错误在全局变量中 */
void handleDoubleFault(tcb_t *tptr, seL4_Fault_t ex1)
#endif
{
#ifdef CONFIG_PRINTING
#ifdef CONFIG_KERNEL_MCS
    printf("Found thread has no fault handler while trying to handle:\n");
    print_fault(current_fault);
#else
    seL4_Fault_t ex2 = current_fault;
    printf("Caught ");
    print_fault(ex2);
    printf("\nwhile trying to handle:\n");
    print_fault(ex1);
#endif
#ifdef CONFIG_DEBUG_BUILD
    printf("\nin thread %p \"%s\" ", tptr, tptr->tcbName);
#endif /* CONFIG_DEBUG_BUILD */

    printf("at address %p\n", (void *)getRestartPC(tptr));
    printf("With stack:\n");
    Arch_userStackTrace(tptr);/*Z 打印线程的栈内容 */
#endif

    setThreadState(tptr, ThreadState_Inactive);
}
