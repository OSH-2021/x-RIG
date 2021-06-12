/*
 * Copyright 2014, General Dynamics C4 Systems
 *
 * SPDX-License-Identifier: GPL-2.0-only
 */

#include <types.h>
#include <string.h>
#include <sel4/constants.h>
#include <kernel/thread.h>
#include <kernel/vspace.h>
#include <machine/registerset.h>
#include <model/statedata.h>
#include <object/notification.h>
#include <object/cnode.h>
#include <object/endpoint.h>
#include <object/tcb.h>
/*Z 设置EP指针的队列头尾 */
static inline void ep_ptr_set_queue(endpoint_t *epptr, tcb_queue_t queue)
{
    endpoint_ptr_set_epQueue_head(epptr, (word_t)queue.head);
    endpoint_ptr_set_epQueue_tail(epptr, (word_t)queue.end);
}

#ifdef CONFIG_KERNEL_MCS
void sendIPC(bool_t blocking, bool_t do_call, word_t badge,
             bool_t canGrant, bool_t canGrantReply, bool_t canDonate, tcb_t *thread, endpoint_t *epptr)
#else
/*Z 发送方调用，将线程待发送的错误消息(优先)或正常消息，及可选的授予能力copy给EP队列第1个接收者。
参数均为发送方相关，结果可能阻塞、静默失败、要求回复并阻塞、成功并激活接收线程 */
void sendIPC(bool_t blocking/*Z 是否阻塞IPC */, bool_t do_call/*Z 是否需回复 */, word_t badge,
             bool_t canGrant, bool_t canGrantReply, tcb_t *thread, endpoint_t *epptr)
#endif
{
    switch (endpoint_ptr_get_state(epptr)) {
    case EPState_Idle:  /*Z 新的IPC过程 */
    case EPState_Send:  /*Z 或已有等待发送的线程 */
        if (blocking) {                             /*Z 如果是阻塞IPC */
            tcb_queue_t queue;

            /* Set thread state to BlockedOnSend *//*Z 则设置发起者线程状态：*/
            thread_state_ptr_set_tsType(&thread->tcbState,  /*Z 阻塞于发送中 */
                                        ThreadState_BlockedOnSend);
            thread_state_ptr_set_blockingObject(            /*Z 阻塞于EP指针 */
                &thread->tcbState, EP_REF(epptr));
            thread_state_ptr_set_blockingIPCBadge(          /*Z 标记 */
                &thread->tcbState, badge);
            thread_state_ptr_set_blockingIPCCanGrant(       /*Z CanGrant */
                &thread->tcbState, canGrant);
            thread_state_ptr_set_blockingIPCCanGrantReply(  /*Z CanGrantReply */
                &thread->tcbState, canGrantReply);
            thread_state_ptr_set_blockingIPCIsCall(         /*Z IsCall */
                &thread->tcbState, do_call);
            /*Z 如果发起者是最终决策对象，则设置调度器重新选择对象(因为阻塞了) */
            scheduleTCB(thread);
            /*Z 将发起者加入队列，更新EP队列，设置发送状态 */
            /* Place calling thread in endpoint queue */
            queue = ep_ptr_get_queue(epptr);/*Z 获取EP指针指向的队列头、尾 */
            queue = tcbEPAppend(thread, queue);/*Z 将线程加入到队列 */
            endpoint_ptr_set_state(epptr, EPState_Send);/*Z 设置EP指针的队列状态 */
            ep_ptr_set_queue(epptr, queue);/*Z 设置EP指针的队列头尾 */
        }/*Z 非阻塞发送时，因为EPState_Idle、EPState_Send都说明没有接收者，因此静默失败 */
        break;

    case EPState_Recv: {/*Z 已有等待接收的线程，就完成一次IPC过程 */
        tcb_queue_t queue;
        tcb_t *dest;

        /* Get the head of the endpoint queue. */
        queue = ep_ptr_get_queue(epptr);
        dest = queue.head;

        /* Haskell error "Receive endpoint queue must not be empty" */
        assert(dest);
        /*Z 将首个线程从EP队列中摘出，更新EP指针 */
        /* Dequeue the first TCB */
        queue = tcbEPDequeue(dest, queue);
        ep_ptr_set_queue(epptr, queue);
        /*Z 如果EP队列为空，则设置EP指针的队列状态为空闲 */
        if (!queue.head) {
            endpoint_ptr_set_state(epptr, EPState_Idle);
        }
        /*Z 将线程待发送的错误消息或正常消息(含可选的授予能力)copy给接收者 */
        /* Do the transfer *//*Z 在页错误时，本来是内核发消息，这里实现为thread向其错误处理线程发消息 */
        doIPCTransfer(thread, epptr, badge, canGrant, dest);

#ifdef CONFIG_KERNEL_MCS    //???????????????????未看
        reply_t *reply = REPLY_PTR(thread_state_get_replyObject(dest->tcbState));
        if (reply) {
            reply_unlink(reply);
        }

        if (do_call ||
            seL4_Fault_ptr_get_seL4_FaultType(&thread->tcbFault) != seL4_Fault_NullFault) {
            if (reply != NULL && (canGrant || canGrantReply)) {
                reply_push(thread, dest, reply, canDonate);
            } else {
                setThreadState(thread, ThreadState_Inactive);
            }
        } else if (canDonate && dest->tcbSchedContext == NULL) {
            schedContext_donate(thread->tcbSchedContext, dest);
        }

        /* blocked threads should have enough budget to get out of the kernel */
        assert(dest->tcbSchedContext == NULL || refill_sufficient(dest->tcbSchedContext, 0));
        assert(dest->tcbSchedContext == NULL || refill_ready(dest->tcbSchedContext));
        setThreadState(dest, ThreadState_Running);
        possibleSwitchTo(dest);
#else   /*Z 取得接收线程的IPC授权能力 */
        bool_t replyCanGrant = thread_state_ptr_get_blockingIPCCanGrant(&dest->tcbState);
        /*Z 设置接收线程状态，以从阻塞状态中解脱出来，并加入调度决策内 */
        setThreadState(dest, ThreadState_Running);
        possibleSwitchTo(dest);
        /*Z 如果要求回复 */
        if (do_call) {
            if (canGrant || canGrantReply) {/*Z 必须要有授权能力 */
                setupCallerCap(thread, dest, replyCanGrant);/*Z 设置发送者为阻塞于接收回复状态，为接收者创建拷贝指向发送者的回复能力 */
            } else {
                setThreadState(thread, ThreadState_Inactive);/*Z 不活跃后怎么办????? */
            }
        }
#endif
        break;
    }
    }
}

#ifdef CONFIG_KERNEL_MCS
void receiveIPC(tcb_t *thread, cap_t cap, bool_t isBlocking, cap_t replyCap)
#else
/*Z Recv类调用：有绑定通知先收通知，无通知收EP消息。*/
void receiveIPC(tcb_t *thread, cap_t cap, bool_t isBlocking)
#endif
{
    endpoint_t *epptr;
    notification_t *ntfnPtr;
    /*Z 必须是EP能力 */
    /* Haskell error "receiveIPC: invalid cap" */
    assert(cap_get_capType(cap) == cap_endpoint_cap);

    epptr = EP_PTR(cap_endpoint_cap_get_capEPPtr(cap));

#ifdef CONFIG_KERNEL_MCS
    reply_t *replyPtr = NULL;
    if (cap_get_capType(replyCap) == cap_reply_cap) {
        replyPtr = REPLY_PTR(cap_reply_cap_get_capReplyPtr(replyCap));
        if (unlikely(replyPtr->replyTCB != NULL && replyPtr->replyTCB != thread)) {
            userError("Reply object already has unexecuted reply!");
            cancelIPC(replyPtr->replyTCB);
        }
    }
#endif
    /*Z 如果线程绑定了通知对象，并且通知已到，则此次IPC接收只处理接收通知 */
    /* Check for anything waiting in the notification */
    ntfnPtr = thread->tcbBoundNotification;
    if (ntfnPtr && notification_ptr_get_state(ntfnPtr) == NtfnState_Active) {
        completeSignal(ntfnPtr, thread);/*Z 接收已发出的通知信号和消息标记，置通知队列不活跃 */
    } else {
        switch (endpoint_ptr_get_state(epptr)) {
        case EPState_Idle:
        case EPState_Recv: {/*Z 发方尚未发出IPC的情况 */
            tcb_queue_t queue;

            if (isBlocking) {/*Z 阻塞接收时加入EP队列 */
                /* Set thread state to BlockedOnReceive */
                thread_state_ptr_set_tsType(&thread->tcbState,
                                            ThreadState_BlockedOnReceive);
                thread_state_ptr_set_blockingObject(
                    &thread->tcbState, EP_REF(epptr));
#ifdef CONFIG_KERNEL_MCS
                thread_state_ptr_set_replyObject(&thread->tcbState, REPLY_REF(replyPtr));
                if (replyPtr) {
                    replyPtr->replyTCB = thread;
                }
#else
                thread_state_ptr_set_blockingIPCCanGrant(
                    &thread->tcbState, cap_endpoint_cap_get_capCanGrant(cap));
#endif
                scheduleTCB(thread);

                /* Place calling thread in endpoint queue */
                queue = ep_ptr_get_queue(epptr);
                queue = tcbEPAppend(thread, queue);
                endpoint_ptr_set_state(epptr, EPState_Recv);
                ep_ptr_set_queue(epptr, queue);
            } else {/*Z 不阻塞接收时静默返回(标记寄存器置0) */
                doNBRecvFailedTransfer(thread);
            }
            break;
        }

        case EPState_Send: {/*Z 发方已发出IPC的情况 */
            tcb_queue_t queue;
            tcb_t *sender;
            word_t badge;
            bool_t canGrant;
            bool_t canGrantReply;
            bool_t do_call;
            /*Z 取出EP队列中的第一个发送者 */
            /* Get the head of the endpoint queue. */
            queue = ep_ptr_get_queue(epptr);
            sender = queue.head;

            /* Haskell error "Send endpoint queue must not be empty" */
            assert(sender);

            /* Dequeue the first TCB */
            queue = tcbEPDequeue(sender, queue);
            ep_ptr_set_queue(epptr, queue);

            if (!queue.head) {
                endpoint_ptr_set_state(epptr, EPState_Idle);
            }
            /*Z 取出发送者状态中的标记、授权 */
            /* Get sender IPC details */
            badge = thread_state_ptr_get_blockingIPCBadge(&sender->tcbState);
            canGrant =
                thread_state_ptr_get_blockingIPCCanGrant(&sender->tcbState);
            canGrantReply =
                thread_state_ptr_get_blockingIPCCanGrantReply(&sender->tcbState);
            /*Z 将sender待发送的错误消息(优先)或正常消息，及可选的授予能力copy给接收者 */
            /* Do the transfer */
            doIPCTransfer(sender, epptr, badge,
                          canGrant, thread);
            /*Z 发送者是否要等待回复 */
            do_call = thread_state_ptr_get_blockingIPCIsCall(&sender->tcbState);

#ifdef CONFIG_KERNEL_MCS
            if (do_call ||
                seL4_Fault_get_seL4_FaultType(sender->tcbFault) != seL4_Fault_NullFault) {
                if ((canGrant || canGrantReply) && replyPtr != NULL) {
                    reply_push(sender, thread, replyPtr, sender->tcbSchedContext != NULL);
                } else {
                    setThreadState(sender, ThreadState_Inactive);
                }
            } else {
                setThreadState(sender, ThreadState_Running);
                possibleSwitchTo(sender);
                assert(sender->tcbSchedContext == NULL || refill_sufficient(sender->tcbSchedContext, 0));
            }
#else
            if (do_call) {/*Z 如果需要回复 */
                if (canGrant || canGrantReply) {/*Z 设置发送者为阻塞于接收回复状态，接收者创建拷贝指向发送者的回复能力 */
                    setupCallerCap(sender, thread, cap_endpoint_cap_get_capCanGrant(cap));
                } else {
                    setThreadState(sender, ThreadState_Inactive);
                }
            } else {/*Z 如果不需要回复，激活发送者 */
                setThreadState(sender, ThreadState_Running);
                possibleSwitchTo(sender);
            }
#endif
            break;
        }
        }
    }
}
/*Z 内核将当前系统调用错误信息发给线程(写消息寄存器(IPC buffer)) */
void replyFromKernel_error(tcb_t *thread)
{
    word_t len;
    word_t *ipcBuffer;

    ipcBuffer = lookupIPCBuffer(true, thread);
    setRegister(thread, badgeRegister, 0);
    len = setMRs_syscall_error(thread, ipcBuffer);

#ifdef CONFIG_KERNEL_INVOCATION_REPORT_ERROR_IPC
    char *debugBuffer = (char *)(ipcBuffer + DEBUG_MESSAGE_START + 1);
    word_t add = strlcpy(debugBuffer, (char *)current_debug_error.errorMessage,
                         DEBUG_MESSAGE_MAXLEN * sizeof(word_t));

    len += (add / sizeof(word_t)) + 1;
#endif

    setRegister(thread, msgInfoRegister, wordFromMessageInfo(
                    seL4_MessageInfo_new(current_syscall_error.type, 0, 0, len)));
}
/*Z 内核将成功信息-全0值发给线程(写消息寄存器(IPC buffer)) */
void replyFromKernel_success_empty(tcb_t *thread)
{
    setRegister(thread, badgeRegister, 0);
    setRegister(thread, msgInfoRegister, wordFromMessageInfo(
                    seL4_MessageInfo_new(0, 0, 0, 0)));
}
/*Z 取消线程的IPC状态：置空要发送的错误、从EP(NF)队列中摘除、删除reply有关对象、(置状态为不活跃) */
void cancelIPC(tcb_t *tptr)
{
    thread_state_t *state = &tptr->tcbState;

#ifdef CONFIG_KERNEL_MCS
    /* cancel ipc cancels all faults *//*Z 置空(丢弃)当前要通过IPC发送的错误 */
    seL4_Fault_NullFault_ptr_new(&tptr->tcbFault);
#endif

    switch (thread_state_ptr_get_tsType(state)) {
    case ThreadState_BlockedOnSend:
    case ThreadState_BlockedOnReceive: {
        /* blockedIPCCancel state */
        endpoint_t *epptr;
        tcb_queue_t queue;
        /*Z 获取阻塞的EP指针 */
        epptr = EP_PTR(thread_state_ptr_get_blockingObject(state));

        /* Haskell error "blockedIPCCancel: endpoint must not be idle" */
        assert(endpoint_ptr_get_state(epptr) != EPState_Idle);
        /*Z 从EP队列中摘出，并更新EP队列 */
        /* Dequeue TCB */
        queue = ep_ptr_get_queue(epptr);
        queue = tcbEPDequeue(tptr, queue);
        ep_ptr_set_queue(epptr, queue);

        if (!queue.head) {
            endpoint_ptr_set_state(epptr, EPState_Idle);
        }

#ifdef CONFIG_KERNEL_MCS
        reply_t *reply = REPLY_PTR(thread_state_get_replyObject(tptr->tcbState));
        if (reply != NULL) {
            reply_unlink(reply);/*Z 删除reply对象与其接收线程之间的关联关系，并置接收线程状态为不活跃 */
        }
#endif
        setThreadState(tptr, ThreadState_Inactive);
        break;
    }

    case ThreadState_BlockedOnNotification:/*Z 取消接收通知并置线程为不活跃状态，更新NF队列 */
        cancelSignal(tptr,
                     NTFN_PTR(thread_state_ptr_get_blockingObject(state)));
        break;

    case ThreadState_BlockedOnReply: {
#ifdef CONFIG_KERNEL_MCS
        reply_remove_tcb(tptr);/*Z 大概是删除线程接收reply的状态  */
#else
        cte_t *slot, *callerCap;

        tptr->tcbFault = seL4_Fault_NullFault_new();

        /* Get the reply cap slot */
        slot = TCB_PTR_CTE_PTR(tptr, tcbReply);
        /*Z 取得被要求回复的CSlot，也就是别人的回复能力 */
        callerCap = CTE_PTR(mdb_node_get_mdbNext(slot->cteMDBNode));
        if (callerCap) {
            /** GHOSTUPD: "(True,
                gs_set_assn cteDeleteOne_'proc (ucast cap_reply_cap))" */
            cteDeleteOne(callerCap);/*Z 删除 */
        }
#endif

        break;
    }
    }
}
/*Z 置空指定EP队列取消其所有的IPC，相关线程重新调度继续往下运行 */
void cancelAllIPC(endpoint_t *epptr)
{
    switch (endpoint_ptr_get_state(epptr)) {
    case EPState_Idle:
        break;

    default: {/*Z EP队列头 */
        tcb_t *thread = TCB_PTR(endpoint_ptr_get_epQueue_head(epptr));
        /*Z EP队列置空、空闲 */
        /* Make endpoint idle */
        endpoint_ptr_set_state(epptr, EPState_Idle);
        endpoint_ptr_set_epQueue_head(epptr, 0);
        endpoint_ptr_set_epQueue_tail(epptr, 0);

        /* Set all blocked threads to restart */
        for (; thread; thread = thread->tcbEPNext) {
#ifdef CONFIG_KERNEL_MCS
            reply_t *reply = REPLY_PTR(thread_state_get_replyObject(thread->tcbState));
            if (reply != NULL) {
                reply_unlink(reply);
            }
            if (seL4_Fault_get_seL4_FaultType(thread->tcbFault) == seL4_Fault_NullFault) {
                setThreadState(thread, ThreadState_Restart);
                possibleSwitchTo(thread);
            } else {
                setThreadState(thread, ThreadState_Inactive);
            }
#else       /*Z 猜测是本次IPC失败，线程重新调度继续往下运行，至于IPC错误应由程序自己处理 */
            setThreadState(thread, ThreadState_Restart);
            SCHED_ENQUEUE(thread);
#endif
        }

        rescheduleRequired();
        break;
    }
    }
}
/*Z 取消EP队列中阻塞于指定标记的线程阻塞 */
void cancelBadgedSends(endpoint_t *epptr, word_t badge)
{
    switch (endpoint_ptr_get_state(epptr)) {
    case EPState_Idle:/*Z 空闲、等待接收的EP队列不存在取消已发送问题 */
    case EPState_Recv:
        break;

    case EPState_Send: {
        tcb_t *thread, *next;
        tcb_queue_t queue = ep_ptr_get_queue(epptr);
        /*Z EP队列置空，状态置空闲 */
        /* this is a de-optimisation for verification
         * reasons. it allows the contents of the endpoint
         * queue to be ignored during the for loop. */
        endpoint_ptr_set_state(epptr, EPState_Idle);
        endpoint_ptr_set_epQueue_head(epptr, 0);
        endpoint_ptr_set_epQueue_tail(epptr, 0);

        for (thread = queue.head; thread; thread = next) {
            word_t b = thread_state_ptr_get_blockingIPCBadge(
                           &thread->tcbState);
            next = thread->tcbEPNext;
#ifdef CONFIG_KERNEL_MCS
            /* senders do not have reply objects in their state, and we are only cancelling sends */
            assert(REPLY_PTR(thread_state_get_replyObject(thread->tcbState)) == NULL);
            if (b == badge) {
                if (seL4_Fault_get_seL4_FaultType(thread->tcbFault) ==
                    seL4_Fault_NullFault) {
                    setThreadState(thread, ThreadState_Restart);
                    possibleSwitchTo(thread);
                } else {
                    setThreadState(thread, ThreadState_Inactive);
                }
                queue = tcbEPDequeue(thread, queue);
            }
#else       /*Z 线程阻塞的标记与要求的标记相符：置重启动，入ready队列，摘出EP队列 */
            if (b == badge) {
                setThreadState(thread, ThreadState_Restart);
                SCHED_ENQUEUE(thread);
                queue = tcbEPDequeue(thread, queue);
            }
#endif
        }
        ep_ptr_set_queue(epptr, queue);

        if (queue.head) {
            endpoint_ptr_set_state(epptr, EPState_Send);
        }

        rescheduleRequired();

        break;
    }

    default:
        fail("invalid EP state");
    }
}

#ifdef CONFIG_KERNEL_MCS
void reorderEP(endpoint_t *epptr, tcb_t *thread)
{
    tcb_queue_t queue = ep_ptr_get_queue(epptr);
    queue = tcbEPDequeue(thread, queue);
    queue = tcbEPAppend(thread, queue);
    ep_ptr_set_queue(epptr, queue);
}
#endif
