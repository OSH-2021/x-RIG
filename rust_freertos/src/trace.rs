// trace.rs - Trace macros.
// This file is created by Fan Jinhao.
// All the trace macros are defined in this file, along with mtCOVERAGE_*
// These macros may be useful when debugging.
// Macros in this file are adapted from FreeRTOS.h

/* Remove any unused trace macros. */
// #[macro_use]

/* Used to perform any necessary initialisation - for example, open a file
into which trace is to be written. */
#[macro_export]
macro_rules! traceSTART {
    () => {
        trace!("Trace starts");
    };
}

/* Use to close a trace, for example close a file into which trace has been
written. */
#[macro_export]
macro_rules! traceEND {
    () => {
        trace!("Trace ends");
    };
}

/* Called after a task has been selected to run.  pxCurrentTCB holds a pointer
to the task control block of the selected task. */
#[macro_export]
macro_rules! traceTASK_SWITCHED_IN {
    () => {
        if let Some(current_task) = get_current_task_handle_wrapped!() {
            trace!("Task {} switched in", current_task.get_name());
        } else {
            warn!("No task switched in");
        }
    };
}

/* Called before stepping the tick count after waking from tickless idle
sleep. */
#[macro_export]
macro_rules! traceINCREASE_TICK_COUNT {
    ($x: expr) => {
        trace!("Tick count increased from {}", $x);
    };
}

/* Called immediately before entering tickless idle. */
#[macro_export]
macro_rules! traceLOW_POWER_IDLE_BEGIN {
    () => {};
}

/* Called when returning to the Idle task after a tickless idle. */
#[macro_export]
macro_rules! traceLOW_POWER_IDLE_END {
    () => {};
}

/* Called before a task has been selected to run.  pxCurrentTCB holds a pointer
to the task control block of the task being switched out. */
#[macro_export]
macro_rules! traceTASK_SWITCHED_OUT {
    () => {
        if let Some(current_task) = get_current_task_handle_wrapped!() {
            trace!("Task {} will be switched out", current_task.get_name());
        }
    };
}

/* Called when a task attempts to take a mutex that is already held by a
lower priority task.  pxTCBOfMutexHolder is a pointer to the TCB of the task
that holds the mutex.  uxInheritedPriority is the priority the mutex holder
will inherit (the priority of the task that is attempting to obtain the
muted. */
#[macro_export]
macro_rules! traceTASK_PRIORITY_INHERIT {
    ($pxTCBOfMutexHolder: expr, $uxInheritedPriority: expr) => {};
}

/* Called when a task releases a mutex, the holding of which had resulted in
the task inheriting the priority of a higher priority task.
pxTCBOfMutexHolder is a pointer to the TCB of the task that is releasing the
mutex.  uxOriginalPriority is the task's configured (base) priority. */
#[macro_export]
macro_rules! traceTASK_PRIORITY_DISINHERIT {
    ($pxTCBOfMutexHolder: expr, $uxOriginalPriority: expr) => {};
}

/* Task is about to block because it cannot read from a
queue/mutex/semaphore.  pxQueue is a pointer to the queue/mutex/semaphore
upon which the read was attempted.  pxCurrentTCB points to the TCB of the
task that attempted the read. */
#[macro_export]
macro_rules! traceBLOCKING_ON_QUEUE_RECEIVE {
    ($pxQueue: expr) => {
        trace!(
            "Blocking task {} because it cannot read from {}.",
            get_current_task_handle!().get_name(),
            $pxQueue.get_queue_number()
        );
    };
}

/* Task is about to block because it cannot write to a
queue/mutex/semaphore.  pxQueue is a pointer to the queue/mutex/semaphore
upon which the write was attempted.  pxCurrentTCB points to the TCB of the
task that attempted the write. */
#[macro_export]
macro_rules! traceBLOCKING_ON_QUEUE_SEND {
    ($pxQueue: expr) => {
        trace!(
            "Blocking task {} because it cannot write to {}.",
            get_current_task_handle!().get_name(),
            $pxQueue.get_queue_number()
        );
    };
}

/* The following event macros are embedded in the kernel API calls. */

#[macro_export]
macro_rules! traceMOVED_TASK_TO_READY_STATE {
    ($pxTCB: expr) => {
        trace!("Moving task {} to ready state.", $pxTCB.get_name());
    };
}

#[macro_export]
macro_rules! tracePOST_MOVED_TASK_TO_READY_STATE {
    ($pxTCB: expr) => {
        trace!("Task {} was moved to ready state.", $pxTCB.get_name());
    };
}

#[macro_export]
macro_rules! traceQUEUE_CREATE {
    ($pxNewQueue: expr) => {
        trace!("Created queue {}", $pxNewQueue.get_queue_number());
    };
}

#[macro_export]
macro_rules! traceQUEUE_CREATE_FAILED {
    ($ucQueueType: expr) => {
        warn!("Queue creation failed.");
    };
}

#[macro_export]
macro_rules! traceCREATE_MUTEX {
    ($pxNewQueue: expr) => {
        trace!("Created mutex {}", $pxNewQueue.0.get_queue_number());
    };
}

#[macro_export]
macro_rules! traceCREATE_MUTEX_FAILED {
    () => {
        warn!("Mutex creation failed.");
    };
}

#[macro_export]
macro_rules! traceGIVE_MUTEX_RECURSIVE {
    ($pxMutex: expr) => {};
}

#[macro_export]
macro_rules! traceGIVE_MUTEX_RECURSIVE_FAILED {
    ($pxMutex: expr) => {};
}

#[macro_export]
macro_rules! traceTAKE_MUTEX_RECURSIVE {
    ($pxMutex: expr) => {};
}

#[macro_export]
macro_rules! traceTAKE_MUTEX_RECURSIVE_FAILED {
    ($pxMutex: expr) => {};
}

#[macro_export]
macro_rules! traceCREATE_COUNTING_SEMAPHORE {
    () => {
        trace!("Created counting semaphore: {}");
    };
}

#[macro_export]
macro_rules! traceCREATE_COUNTING_SEMAPHORE_FAILED {
    () => {
        trace!("Counting semaphore creation failed.");
    };
}

#[macro_export]
macro_rules! traceQUEUE_SEND {
    ($pxQueue: expr) => {
        trace!("Sending to queue {}", $pxQueue.get_queue_number());
    };
}

#[macro_export]
macro_rules! traceQUEUE_SEND_FAILED {
    ($pxQueue: expr) => {
        warn!("Queue send failed!");
    };
}

#[macro_export]
macro_rules! traceQUEUE_RECEIVE {
    ($pxQueue: expr) => {
        trace!("Receiving from queue {}", $pxQueue.get_queue_number());
    };
}

#[macro_export]
macro_rules! traceQUEUE_PEEK {
    ($pxQueue: expr) => {
        trace!("Peeking from queue {}", $pxQueue.get_queue_number());
    };
}

#[macro_export]
macro_rules! traceQUEUE_PEEK_FROM_ISR {
    ($pxQueue: expr) => {};
}

#[macro_export]
macro_rules! traceQUEUE_RECEIVE_FAILED {
    ($pxQueue: expr) => {};
}

#[macro_export]
macro_rules! traceQUEUE_SEND_FROM_ISR {
    ($pxQueue: expr) => {};
}

#[macro_export]
macro_rules! traceQUEUE_SEND_FROM_ISR_FAILED {
    ($pxQueue: expr) => {};
}

#[macro_export]
macro_rules! traceQUEUE_RECEIVE_FROM_ISR {
    ($pxQueue: expr) => {};
}

#[macro_export]
macro_rules! traceQUEUE_RECEIVE_FROM_ISR_FAILED {
    ($pxQueue: expr) => {};
}

#[macro_export]
macro_rules! traceQUEUE_PEEK_FROM_ISR_FAILED {
    ($pxQueue: expr) => {};
}

#[macro_export]
macro_rules! traceQUEUE_DELETE {
    ($pxQueue: expr) => {
        trace!("Deleting queue {}", pxQueue.get_queue_number());
    };
}

/* This macro is defined in port.rs
#[macro_export]
macro_rules! traceTASK_CREATE {
    ($pxNewTCB: expr) => {};
}
*/

#[macro_export]
macro_rules! traceTASK_CREATE_FAILED {
    () => {
        warn!("Task creation failed!");
    };
}

/* This macro is defined in port.rs
#[macro_export]
macro_rules! traceTASK_DELETE {
    ($pxTaskToDelete: expr) => {};
}
*/

#[macro_export]
macro_rules! traceTASK_DELAY_UNTIL {
    ($x: expr) => {};
}

#[macro_export]
macro_rules! traceTASK_DELAY {
    () => {};
}

#[macro_export]
macro_rules! traceTASK_PRIORITY_SET {
    ($pxTask: expr, $uxNewPriority: expr) => {};
}

#[macro_export]
macro_rules! traceTASK_SUSPEND {
    ($pxTaskToSuspend: expr) => {};
}

#[macro_export]
macro_rules! traceTASK_RESUME {
    ($pxTaskToResume: expr) => {};
}

#[macro_export]
macro_rules! traceTASK_RESUME_FROM_ISR {
    ($pxTaskToResume: expr) => {};
}

#[macro_export]
macro_rules! traceTASK_INCREMENT_TICK {
    ($xTickCount: expr) => {};
}

#[macro_export]
macro_rules! traceTIMER_CREATE {
    ($pxNewTimer: expr) => {};
}

#[macro_export]
macro_rules! traceTIMER_CREATE_FAILED {
    () => {};
}

#[macro_export]
macro_rules! traceTIMER_COMMAND_SEND {
    ($xTimer: expr, $xMessageID: expr, $xMessageValueValue: expr, $xReturn: expr) => {};
}

#[macro_export]
macro_rules! traceTIMER_EXPIRED {
    ($pxTimer: expr) => {};
}

#[macro_export]
macro_rules! traceTIMER_COMMAND_RECEIVED {
    ($pxTimer: expr, $xMessageID: expr, $xMessageValue: expr) => {};
}

#[macro_export]
macro_rules! traceMALLOC {
    ($pvAddress: expr, $uiSize: expr) => {};
}

#[macro_export]
macro_rules! traceFREE {
    ($pvAddress: expr, $uiSize: expr) => {};
}

#[macro_export]
macro_rules! traceEVENT_GROUP_CREATE {
    ($xEventGroup: expr) => {};
}

#[macro_export]
macro_rules! traceEVENT_GROUP_CREATE_FAILED {
    () => {};
}

#[macro_export]
macro_rules! traceEVENT_GROUP_SYNC_BLOCK {
    ($xEventGroup: expr, $uxBitsToSet: expr, $uxBitsToWaitFor: expr) => {};
}

#[macro_export]
macro_rules! traceEVENT_GROUP_SYNC_END {
    ($xEventGroup: expr, $uxBitsToSet: expr, $uxBitsToWaitFor: expr, $xTimeoutOccurred: expr) => {};
}

#[macro_export]
macro_rules! traceEVENT_GROUP_WAIT_BITS_BLOCK {
    ($xEventGroup: expr, $uxBitsToWaitFor: expr) => {};
}

#[macro_export]
macro_rules! traceEVENT_GROUP_WAIT_BITS_END {
    ($xEventGroup: expr, $uxBitsToWaitFor: expr, $xTimeoutOccurred: expr) => {};
}

#[macro_export]
macro_rules! traceEVENT_GROUP_CLEAR_BITS {
    ($xEventGroup: expr, $uxBitsToClear: expr) => {};
}

#[macro_export]
macro_rules! traceEVENT_GROUP_CLEAR_BITS_FROM_ISR {
    ($xEventGroup: expr, $uxBitsToClear: expr) => {};
}

#[macro_export]
macro_rules! traceEVENT_GROUP_SET_BITS {
    ($xEventGroup: expr, $uxBitsToSet: expr) => {};
}

#[macro_export]
macro_rules! traceEVENT_GROUP_SET_BITS_FROM_ISR {
    ($xEventGroup: expr, $uxBitsToSet: expr) => {};
}

#[macro_export]
macro_rules! traceEVENT_GROUP_DELETE {
    ($xEventGroup: expr) => {};
}

#[macro_export]
macro_rules! tracePEND_FUNC_CALL {
    ($xFunctionToPend: expr, $pvParameter1: expr, $ulParameter2: expr, $ret: expr) => {};
}

#[macro_export]
macro_rules! tracePEND_FUNC_CALL_FROM_ISR {
    ($xFunctionToPend: expr, $pvParameter1: expr, $ulParameter2: expr, $ret: expr) => {};
}

#[macro_export]
macro_rules! traceQUEUE_REGISTRY_ADD {
    ($xQueue: expr, $pcQueueName: expr) => {};
}

#[macro_export]
macro_rules! traceTASK_NOTIFY_TAKE_BLOCK {
    () => {};
}

#[macro_export]
macro_rules! traceTASK_NOTIFY_TAKE {
    () => {};
}

#[macro_export]
macro_rules! traceTASK_NOTIFY_WAIT_BLOCK {
    () => {
        trace!("Task is block out of waiting for notification.")
    };
}

#[macro_export]
macro_rules! traceTASK_NOTIFY_WAIT {
    () => {};
}

#[macro_export]
macro_rules! traceTASK_NOTIFY {
    () => {};
}

#[macro_export]
macro_rules! traceTASK_NOTIFY_FROM_ISR {
    () => {};
}

#[macro_export]
macro_rules! traceTASK_NOTIFY_GIVE_FROM_ISR {
    () => {};
}

#[macro_export]
macro_rules! mtCOVERAGE_TEST_MARKER {
    () => {};
}

#[macro_export]
macro_rules! mtCOVERAGE_TEST_DELAY {
    () => {};
}

#[macro_export]
macro_rules! traceSTREAM_BUFFER_RESET {
    () => {};
}

#[macro_export]
macro_rules! traceSTREAM_BUFFER_SEND {
    () => { 
        trace!("sender is sending....");
    };
}

#[macro_export]
macro_rules! traceSTREAM_BUFFER_SEND_FAILED {
    ($xStreamBuffer:expr) => {
        trace!("The send fails!");
    };
}


#[macro_export]
macro_rules! traceBLOCKING_ON_STREAM_BUFFER_RECEIVE {
    ($xStreamBuffer:expr) => {
        trace!("Receiver is waiting for message....");
    };
}


#[macro_export]
macro_rules! traceSTREAM_BUFFER_RECEIVE {
    ($xReturn:expr) => { 
        trace!("The receiver receive {} byte(s)", $xReturn);
    };
}


#[macro_export]
macro_rules! traceSTREAM_BUFFER_RECEIVE_FAILED {
    () => {
        trace!("The receive fails!");
    };
}
