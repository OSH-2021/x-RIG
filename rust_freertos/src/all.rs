// config.rs - Macros starting with "config" who evaluate to a numeric value.
// This file is adapted from FreeRTOSConfig.h

// TODO: Provide configASSERT() (or not?).
#[macro_export]
macro_rules! configTICK_RATE_HZ {
    () => {
        1000 as port::TickType
    };
}

#[macro_export]
macro_rules! configMINIMAL_STACK_SIZE {
    () => {
        64
    };
}

#[macro_export]
macro_rules! configTOTAL_HEAP_SIZE {
    () => {
        64 * 1024 as usize
    };
}

#[macro_export]
macro_rules! configMAX_TASK_NAME_LEN {
    () => {
        16
    };
}

#[macro_export]
macro_rules! configQUEUE_REGISTRY_SIZE {
    () => {
        20
    };
}

#[macro_export]
macro_rules! configMAX_PRIORITIES {
    () => {
        10
    };
}

#[macro_export]
macro_rules! configTIMER_TASK_PRIORITY {
    () => {
        configMAX_PRIORITIES!() - 1
    };
}

#[macro_export]
macro_rules! configTIMER_TASK_STACK_DEPTH {
    () => {
        configMINIMAL_STACK_SIZE * 2
    };
}

#[macro_export]
macro_rules! configEXPECTED_IDLE_TIME_BEFORE_SLEEP {
    () => {
        2
    };
}
// DO NOT CHANGE THIS FILE!

// ffi.rs - Foreign function interface.
// This file is created by Fan Jinhao.
// It's meant to be an interface for C functions to call Rust functions.

use crate::kernel;
use crate::port::BaseType;
use crate::projdefs::{pdFALSE, pdTRUE};
use crate::*;

pub type xTaskHandle = *mut ::std::os::raw::c_void;

#[no_mangle]
extern "C" fn xTaskGetCurrentTaskHandle() -> xTaskHandle {
    trace!("xTaskGetCurrentTaskHandle() called from ffi!");
    get_current_task_handle!().as_raw()
}

#[no_mangle]
extern "C" fn xTaskIncrementTick() -> BaseType {
    trace!("xTaskIncrementTick() called from ffi!");
    if kernel::task_increment_tick() {
        info!("task_increment_tick() returned true, need context switch");
        pdTRUE
    } else {
        info!("task_increment_tick() returned false, do not need context switch");
        pdFALSE
    }
}

#[no_mangle]
extern "C" fn vTaskSwitchContext() {
    trace!("vTaskSwitchContext() called from ffi!");
    kernel::task_switch_context();
}

#[no_mangle]
extern "C" fn vTaskSuspendAll() {
    trace!("vTaskSuspendAll() called from ffi!");
    kernel::task_suspend_all();
}

#[no_mangle]
extern "C" fn xTaskResumeAll() -> BaseType {
    trace!("xTaskResumeAll() called from ffi!");
    if kernel::task_resume_all() {
        info!("task_resume_all() returned true");
        pdTRUE
    } else {
        info!("task_resume_all() returned false");
        pdFALSE
    }
}
// kernel.rs, FreeRTOS scheduler control APIs.
// This file is created by Fan Jinhao.
// Functions defined in this file are explained in Chapter 9 and 10.
use crate::list;
use crate::port::UBaseType;
use crate::projdefs::pdFALSE;
use crate::task_control::{TaskHandle, TCB};
use crate::task_global::*;
use crate::*; // TODO: Is this line necessary?
              // use crate::task_control::TCB;

/* Definitions returned by xTaskGetSchedulerState().
 * The originial definitons are C constants, we changed them to enums.
 */
pub enum SchedulerState {
    NotStarted,
    Suspended,
    Running,
}

/// Macro for forcing a context switch.
///
/// * Implemented by: Fan Jinhao.
/// * C implementation:
///
/// # Arguments
///
///
/// # Return
///
/// Nothing
#[macro_export]
macro_rules! taskYIELD {
    () => {
        portYIELD!()
    };
}

#[macro_export]
macro_rules! taskYIELD_IF_USING_PREEMPTION {
    () => {
        #[cfg(feature = "configUSE_PREEMPTION")]
        portYIELD_WITHIN_API!();
    };
}

/// Macro to mark the start of a critical code region.  Preemptive context
/// switches cannot occur when in a critical region.
///
/// NOTE: This may alter the stack (depending on the portable implementation)
/// so must be used with care!
/// * Implemented by: Fan Jinhao.
/// * C implementation:
///
/// # Arguments
///
///
/// # Return
///
/// Nothing
#[macro_export]
macro_rules! taskENTER_CRITICAL {
    () => {
        portENTER_CRITICAL!()
    };
}

#[macro_export]
macro_rules! taskENTER_CRITICAL_FROM_ISR {
    () => {
        portSET_INTERRUPT_MASK_FROM_ISR!()
    };
}

/// Macro to mark the end of a critical code region.  Preemptive context
/// switches cannot occur when in a critical region.
///
/// NOTE: This may alter the stack (depending on the portable implementation)
/// so must be used with care!
/// * Implemented by: Fan Jinhao.
/// * C implementation:
///
/// # Arguments
///
///
/// # Return
///
/// Nothing
#[macro_export]
macro_rules! taskEXIT_CRITICAL {
    () => {
        portEXIT_CRITICAL!()
    };
}

#[macro_export]
macro_rules! taskEXIT_CRITICAL_FROM_ISR {
    ($x: expr) => {
        portCLEAR_INTERRUPT_MASK_FROM_ISR!($x)
    };
}

/// Macro to disable all maskable interrupts.
/// * Implemented by: Fan Jinhao.
/// * C implementation: task.h
///
/// # Arguments
///
/// # Return
///
/// Nothing

#[macro_export]
macro_rules! taskDISABLE_INTERRUPTS {
    () => {
        portDISABLE_INTERRUPTS!()
    };
}

/// Macro to enable microcontroller interrupts.
///
/// * Implemented by: Fan Jinhao.
/// * C implementation: task.h
///
/// # Arguments
///
/// # Return
///
/// Nothing

#[macro_export]
macro_rules! taskENABLE_INTERRUPTS {
    () => {
        portENABLE_INTERRUPTS!()
    };
}

///
/// Starts the real time kernel tick processing.  After calling the kernel
/// has control over which tasks are executed and when.
///
/// See the demo application file main.c for an example of creating
/// tasks and starting the kernel.
///
/// * Implemented by: Fan Jinhao.
/// * C implementation:
///
/// # Arguments
///
///
/// # Return
///
/// Nothing
///
pub fn task_start_scheduler() {
    create_idle_task();

    #[cfg(feature = "configUSE_TIMERS")]
    create_timer_task();

    initialize_scheduler();
}

/// The fist part of task_start_scheduler(), creates the idle task.
/// Will panic if task creation fails.
/// * Implemented by: Fan Jinhao.
/// * C implementation: tasks.c 1831-1866
///
/// # Arguments
///
///
/// # Return
///
/// Nothing
pub fn create_idle_task() -> TaskHandle {
    println!("number: {}", get_current_number_of_tasks!());
    let idle_task_fn = || {
        loop {
            trace!("Idle Task running");
            /* THIS IS THE RTOS IDLE TASK - WHICH IS CREATED AUTOMATICALLY WHEN THE
            SCHEDULER IS STARTED. */

            /* See if any tasks have deleted themselves - if so then the idle task
            is responsible for freeing the deleted task's TCB and stack. */
            check_tasks_waiting_termination();

            /* If we are not using preemption we keep forcing a task switch to
            see if any other task has become available.  If we are using
            preemption we don't need to do this as any task becoming available
            will automatically get the processor anyway. */
            #[cfg(not(feature = "configUSE_PREEMPTION"))]
            taskYIELD!();

            {
                #![cfg(all(feature = "configUSE_PREEMPTION", feature = "configIDLE_SHOULD_YIELD"))]
                /* When using preemption tasks of equal priority will be
                timesliced.  If a task that is sharing the idle priority is ready
                to run then the idle task should yield before the end of the
                timeslice.

                A critical region is not required here as we are just reading from
                the list, and an occasional incorrect value will not matter.  If
                the ready list at the idle priority contains more than one task
                then a task other than the idle task is ready to execute. */
                if list::current_list_length(&READY_TASK_LISTS[0]) > 1 {
                    taskYIELD!();
                } else {
                    mtCOVERAGE_TEST_MARKER!();
                }
            }

            {
                #![cfg(feature = "configUSE_IDLE_HOOK")]
                // TODO: Use IdleHook
                // extern void vApplicationIdleHook( void );

                /* Call the user defined function from within the idle task.  This
                allows the application designer to add background functionality
                without the overhead of a separate task.
                NOTE: vApplicationIdleHook() MUST NOT, UNDER ANY CIRCUMSTANCES,
                CALL A FUNCTION THAT MIGHT BLOCK. */
                // vApplicationIdleHook();
                trace!("Idle Task running");
            }
        }
    };

    TCB::new()
        .priority(0)
        .name("Idle")
        .initialise(idle_task_fn)
        .unwrap_or_else(|err| panic!("Idle task creation failed with error: {:?}", err))
}

fn check_tasks_waiting_termination() {
    // TODO: Wait for task_delete.
}

/// The second (optional) part of task_start_scheduler(),
/// creates the timer task. Will panic if task creation fails.
/// * Implemented by: Fan Jinhao.
/// * C implementation: tasks.c 1868-1879
///
/// # Arguments
///
///
/// # Return
///
/// Nothing
fn create_timer_task() {
    // TODO: This function relies on the software timer, which we may not implement.
    // timer::create_timer_task()
    // On fail, panic!("No enough heap space to allocate timer task.");
}

/// The third part of task_step_scheduler, do some initialziation
/// and call port_start_scheduler() to set up the timer tick.
///
/// * Implemented by: Fan Jinhao.
/// * C implementation: tasks.c 1881-1918.
///
/// # Arguments
///
///
/// # Return
///
/// Nothing
fn initialize_scheduler() {
    /* Interrupts are turned off here, to ensure a tick does not occur
    before or during the call to xPortStartScheduler().  The stacks of
    the created tasks contain a status word with interrupts switched on
    so interrupts will automatically get re-enabled when the first task
    starts to run. */
    portDISABLE_INTERRUPTS!();

    // TODO: NEWLIB

    set_next_task_unblock_time!(port::portMAX_DELAY);
    set_scheduler_running!(true);
    set_tick_count!(0);

    /* If configGENERATE_RUN_TIME_STATS is defined then the following
    macro must be defined to configure the timer/counter used to generate
    the run time counter time base. */
    portCONFIGURE_TIMER_FOR_RUN_TIME_STATS!();

    /* Setting up the timer tick is hardware specific and thus in the
    portable interface. */
    if port::port_start_scheduler() != pdFALSE {
        /* Should not reach here as if the scheduler is running the
        function will not return. */
    } else {
        // TODO: Maybe a trace here?
        /* Should only reach here if a task calls xTaskEndScheduler(). */
    }
}

/// NOTE:  At the time of writing only the x86 real mode port, which runs on a PC
/// in place of DOS, implements this function.
///
/// Stops the real time kernel tick.  All created tasks will be automatically
/// deleted and multitasking (either preemptive or cooperative) will
/// stop.  Execution then resumes from the point where vTaskStartScheduler ()
/// was called, as if vTaskStartScheduler () had just returned.
///
/// See the demo application file main. c in the demo/PC directory for an
/// example that uses vTaskEndScheduler ().
///
/// vTaskEndScheduler () requires an exit function to be defined within the
/// portable layer (see vPortEndScheduler () in port. c for the PC port).  This
/// performs hardware specific operations such as stopping the kernel tick.
///
/// vTaskEndScheduler () will cause all of the resources allocated by the
/// kernel to be freed - but will not free resources allocated by application
/// tasks.
///
/// * Implemented by: Fan Jinhao.
/// * C implementation:
///
/// # Arguments
///
///
/// # Return
///
/// Nothing

pub fn task_end_scheduler() {
    /* Stop the scheduler interrupts and call the portable scheduler end
    routine so the original ISRs can be restored if necessary.  The port
    layer must ensure interrupts enable bit is left in the correct state. */
    portDISABLE_INTERRUPTS!();
    set_scheduler_running!(false);
    port::port_end_scheduler();
}

/// Suspends the scheduler without disabling interrupts.  Context switches will
/// not occur while the scheduler is suspended.
///
/// After calling vTaskSuspendAll () the calling task will continue to execute
/// without risk of being swapped out until a call to xTaskResumeAll () has been
/// made.
///
/// API functions that have the potential to cause a context switch (for example,
/// vTaskDelayUntil(), xQueueSend(), etc.) must not be called while the scheduler
/// is suspended.
///
/// * Implemented by: Fan Jinhao.
/// * C implementation:
///
/// # Arguments
///
///
/// # Return
///
/// Nothing
pub fn task_suspend_all() {
    /* A critical section is not required as the variable is of type
    BaseType_t.  Please read Richard Barry's reply in the following link to a
    post in the FreeRTOS support forum before reporting this as a bug! -
    http://goo.gl/wu4acr */

    // Increment SCHEDULER_SUSPENDED.
    set_scheduler_suspended!(get_scheduler_suspended!() + 1);
}

/// Resumes scheduler activity after it was suspended by a call to
/// vTaskSuspendAll().
///
/// xTaskResumeAll() only resumes the scheduler.  It does not unsuspend tasks
/// that were previously suspended by a call to vTaskSuspend().
/// * Implemented by: Fan Jinhao.
/// * C implementation:
///
/// # Arguments
///
///
/// # Return
///
/// If resuming the scheduler caused a context switch then true is
/// returned, otherwise false is returned.
pub fn task_resume_all() -> bool {
    trace!("resume_all called!");
    let mut already_yielded = false;

    // TODO: This is a recoverable error, use Result<> instead.
    assert!(
        get_scheduler_suspended!() > pdFALSE as UBaseType,
        "The call to task_resume_all() does not match \
         a previous call to vTaskSuspendAll()."
    );

    /* It is possible that an ISR caused a task to be removed from an event
    list while the scheduler was suspended.  If this was the case then the
    removed task will have been added to the xPendingReadyList.  Once the
    scheduler has been resumed it is safe to move all the pending ready
    tasks from this list into their appropriate ready list. */
    taskENTER_CRITICAL!();
    {
        // Decrement SCHEDULER_SUSPENDED.
        set_scheduler_suspended!(get_scheduler_suspended!() - 1);
        println!(
            "get_current_number_of_tasks: {}",
            get_current_number_of_tasks!()
        );
        if get_scheduler_suspended!() == pdFALSE as UBaseType {
            if get_current_number_of_tasks!() > 0 {
                trace!(
                    "Current number of tasks is: {}, move tasks to ready list.",
                    get_current_number_of_tasks!()
                );
                /* Move any readied tasks from the pending list into the
                appropriate ready list. */
                if move_tasks_to_ready_list() {
                    /* A task was unblocked while the scheduler was suspended,
                    which may have prevented the next unblock time from being
                    re-calculated, in which case re-calculate it now.  Mainly
                    important for low power tickless implementations, where
                    this can prevent an unnecessary exit from low power
                    state. */
                    reset_next_task_unblock_time();
                }

                /* If any ticks occurred while the scheduler was suspended then
                they should be processed now.  This ensures the tick count does
                not slip, and that any delayed tasks are resumed at the correct
                time. */
                process_pended_ticks();

                if get_yield_pending!() {
                    {
                        #![cfg(feature = "configUSE_PREEMPTION")]
                        already_yielded = true;
                    }

                    taskYIELD_IF_USING_PREEMPTION!();
                } else {
                    mtCOVERAGE_TEST_MARKER!();
                }
            }
        } else {
            mtCOVERAGE_TEST_MARKER!();
        }
    }

    trace!("Already yielded is {}", already_yielded);
    already_yielded
}

fn move_tasks_to_ready_list() -> bool {
    let mut has_unblocked_task = false;
    while !list::list_is_empty(&PENDING_READY_LIST) {
        trace!("PEDING_LIST not empty");
        has_unblocked_task = true;
        let task_handle = list::get_owner_of_head_entry(&PENDING_READY_LIST);
        let event_list_item = task_handle.get_event_list_item();
        let state_list_item = task_handle.get_state_list_item();

        list::list_remove(state_list_item);
        list::list_remove(event_list_item);

        task_handle.add_task_to_ready_list().unwrap();

        /* If the moved task has a priority higher than the current
        task then a yield must be performed. */
        if task_handle.get_priority() >= get_current_task_priority!() {
            set_yield_pending!(true);
        } else {
            mtCOVERAGE_TEST_MARKER!();
        }
    }
    has_unblocked_task
}

fn reset_next_task_unblock_time() {
    if list::list_is_empty(&DELAYED_TASK_LIST) {
        /* The new current delayed list is empty.  Set xNextTaskUnblockTime to
        the maximum possible value so it is	extremely unlikely that the
        if( xTickCount >= xNextTaskUnblockTime ) test will pass until
        there is an item in the delayed list. */
        set_next_task_unblock_time!(port::portMAX_DELAY);
    } else {
        /* The new current delayed list is not empty, get the value of
        the item at the head of the delayed list.  This is the time at
        which the task at the head of the delayed list should be removed
        from the Blocked state. */
        let task_handle = list::get_owner_of_head_entry(&DELAYED_TASK_LIST);
        set_next_task_unblock_time!(list::get_list_item_value(
            &task_handle.get_state_list_item()
        ));
    }
}

fn process_pended_ticks() {
    trace!("Processing pended ticks");
    let mut pended_counts = get_pended_ticks!();

    if pended_counts > 0 {
        loop {
            if task_increment_tick() {
                set_yield_pending!(true);
            } else {
                mtCOVERAGE_TEST_MARKER!();
            }

            pended_counts -= 1;

            if pended_counts <= 0 {
                break;
            }
        }

        set_pended_ticks!(0);
    } else {
        mtCOVERAGE_TEST_MARKER!();
    }
}

/// Only available when configUSE_TICKLESS_IDLE is set to 1.
/// If tickless mode is being used, or a low power mode is implemented, then
/// the tick interrupt will not execute during idle periods.  When this is the
/// case, the tick count value maintained by the scheduler needs to be kept up
/// to date with the actual execution time by being skipped forward by a time
/// equal to the idle period.
///
/// * Implemented by: Fan Jinhao.
/// * C implementation:
///
/// # Arguments
///
///
/// # Return
///
/// Nothing
#[cfg(feature = "configUSE_TICKLESS_IDLE")]
pub fn task_step_tick(ticks_to_jump: TickType) {
    /* Correct the tick count value after a period during which the tick
    was suppressed.  Note this does *not* call the tick hook function for
    each stepped tick. */
    let cur_tick_count = get_tick_count!(); // NOTE: Is this a bug in FreeRTOS?
    let next_task_unblock_time = get_next_task_unblock_time!();

    // TODO: Add explanations about this assertion.
    assert!(cur_tick_count + ticks_to_jump <= next_task_unblock_time);

    set_tick_count!(cur_tick_count + ticks_to_jump);

    traceINCREASE_TICK_COUNT!(xTicksToJump);
}

/// THIS FUNCTION MUST NOT BE USED FROM APPLICATION CODE.  IT IS ONLY
/// INTENDED FOR USE WHEN IMPLEMENTING A PORT OF THE SCHEDULER AND IS
/// AN INTERFACE WHICH IS FOR THE EXCLUSIVE USE OF THE SCHEDULER.
///
/// Sets the pointer to the current TCB to the TCB of the highest priority task
/// that is ready to run.
///
/// * Implemented by: Fan Jinhao.
/// * C implementation:
///
/// # Arguments
///
/// # Return
///
/// Nothing
pub fn task_switch_context() {
    if get_scheduler_suspended!() > pdFALSE as UBaseType {
        /* The scheduler is currently suspended - do not allow a context
        switch. */
        set_yield_pending!(true);
    } else {
        set_yield_pending!(false);
        traceTASK_SWITCHED_OUT!();

        #[cfg(feature = "configGENERATE_RUN_TIME_STATS")]
        generate_context_switch_stats();

        /* Check for stack overflow, if configured. */
        taskCHECK_FOR_STACK_OVERFLOW!();

        /* Select a new task to run using either the generic Rust or port
        optimised asm code. */
        task_select_highest_priority_task();
        traceTASK_SWITCHED_IN!();

        // TODO: configUSE_NEWLIB_REENTRANT
    }
}

/* If configUSE_PORT_OPTIMISED_TASK_SELECTION is 0 then task selection is
performed in a generic way that is not optimised to any particular
microcontroller architecture. */
#[cfg(not(feature = "configUSE_PORT_OPTIMISED_TASK_SELECTION"))]
fn task_select_highest_priority_task() {
    let mut top_priority: UBaseType = get_top_ready_priority!();

    /* Find the highest priority queue that contains ready tasks. */
    while list::list_is_empty(&READY_TASK_LISTS[top_priority as usize]) {
        assert!(top_priority > 0, "No task found with a non-zero priority");
        top_priority -= 1;
    }

    /* listGET_OWNER_OF_NEXT_ENTRY indexes through the list, so the tasks of
    the same priority get an equal share of the processor time. */
    let next_task = list::get_owner_of_next_entry(&READY_TASK_LISTS[top_priority as usize]);

    trace!("Next task is {}", next_task.get_name());
    set_current_task_handle!(next_task);

    set_top_ready_priority!(top_priority);
}

#[cfg(feature = "configGENERATE_RUN_TIME_STATS")]
fn generate_context_switch_stats() {
    /*
    #ifdef portALT_GET_RUN_TIME_COUNTER_VALUE
    portALT_GET_RUN_TIME_COUNTER_VALUE( ulTotalRunTime );
    #else
    ulTotalRunTime = portGET_RUN_TIME_COUNTER_VALUE();
    #endif
    */
    let total_run_time = portGET_RUN_TIME_COUNTER_VALUE!() as u32;
    trace!("Total runtime: {}", total_run_time);
    set_total_run_time!(total_run_time);

    /* Add the amount of time the task has been running to the
    accumulated time so far.  The time the task started running was
    stored in ulTaskSwitchedInTime.  Note that there is no overflow
    protection here so count values are only valid until the timer
    overflows.  The guard against negative values is to protect
    against suspect run time stat counter implementations - which
    are provided by the application, not the kernel. */
    let task_switched_in_time = get_task_switch_in_time!();
    if total_run_time > task_switched_in_time {
        let current_task = get_current_task_handle!();
        let old_run_time = current_task.get_run_time();
        current_task.set_run_time(old_run_time + total_run_time - task_switched_in_time);
    } else {
        mtCOVERAGE_TEST_MARKER!();
    }
    set_task_switch_in_time!(total_run_time);
}

pub fn task_increment_tick() -> bool {
    // TODO: tasks.c 2500
    let mut switch_required = false;

    /* Called by the portable layer each time a tick interrupt occurs.
    Increments the tick then checks to see if the new tick value will cause any
    tasks to be unblocked. */
    traceTASK_INCREMENT_TICK!(get_tick_count!());

    trace!("SCHEDULER_SUSP is {}", get_scheduler_suspended!());
    if get_scheduler_suspended!() == pdFALSE as UBaseType {
        /* Minor optimisation.  The tick count cannot change in this
        block. */
        let const_tick_count = get_tick_count!() + 1;

        /* Increment the RTOS tick, switching the delayed and overflowed
        delayed lists if it wraps to 0. */
        set_tick_count!(const_tick_count);

        if const_tick_count == 0 {
            switch_delayed_lists!();
        } else {
            mtCOVERAGE_TEST_MARKER!();
        }

        /* See if this tick has made a timeout expire.  Tasks are stored in
        the	queue in the order of their wake time - meaning once one task
        has been found whose block time has not expired there is no need to
        look any further down the list. */
        if const_tick_count >= get_next_task_unblock_time!() {
            trace!("UNBLOCKING!");
            loop {
                if list::list_is_empty(&DELAYED_TASK_LIST) {
                    /* The delayed list is empty.  Set xNextTaskUnblockTime
                    to the maximum possible value so it is extremely
                    unlikely that the
                    if( xTickCount >= xNextTaskUnblockTime ) test will pass
                    next time through. */
                    set_next_task_unblock_time!(port::portMAX_DELAY);
                    break;
                } else {
                    /* The delayed list is not empty, get the value of the
                    item at the head of the delayed list.  This is the time
                    at which the task at the head of the delayed list must
                    be removed from the Blocked state. */
                    let delay_head_entry_owner = list::get_owner_of_head_entry(&DELAYED_TASK_LIST);
                    let task_handle = delay_head_entry_owner;
                    let state_list_item = task_handle.get_state_list_item();
                    let event_list_item = task_handle.get_event_list_item();
                    let item_value = list::get_list_item_value(&state_list_item);

                    if const_tick_count < item_value {
                        /* It is not time to unblock this item yet, but the
                        item value is the time at which the task at the head
                        of the blocked list must be removed from the Blocked
                        state -	so record the item value in
                        xNextTaskUnblockTime. */
                        set_next_task_unblock_time!(item_value);
                        break;
                    } else {
                        mtCOVERAGE_TEST_MARKER!();
                    }

                    /* It is time to remove the item from the Blocked state. */
                    list::list_remove(state_list_item.clone());

                    /* Is the task waiting on an event also?  If so remove
                    it from the event list. */
                    if list::get_list_item_container(&event_list_item).is_some() {
                        list::list_remove(event_list_item.clone());
                    }
                    /* Place the unblocked task into the appropriate ready
                    list. */
                    task_handle.add_task_to_ready_list().unwrap();

                    /* A task being unblocked cannot cause an immediate
                    context switch if preemption is turned off. */
                    {
                        #![cfg(feature = "configUSE_PREEMPTION")]
                        /* Preemption is on, but a context switch should
                        only be performed if the unblocked task has a
                        priority that is equal to or higher than the
                        currently executing task. */
                        if task_handle.get_priority() >= get_current_task_priority!() {
                            switch_required = true;
                        } else {
                            mtCOVERAGE_TEST_MARKER!();
                        }
                    }
                }
            }
        }

        /* Tasks of equal priority to the currently running task will share
        processing time (time slice) if preemption is on, and the application
        writer has not explicitly turned time slicing off. */
        {
            #![cfg(all(feature = "configUSE_PREEMPTION", feature = "configUSE_TIME_SLICING"))]
            let cur_task_pri = get_current_task_priority!();

            if list::current_list_length(&READY_TASK_LISTS[cur_task_pri as usize]) > 1 {
                switch_required = true;
            } else {
                mtCOVERAGE_TEST_MARKER!();
            }
        }

        {
            #![cfg(feature = "configUSE_TICK_HOOK")]
            /* Guard against the tick hook being called when the pended tick
            count is being unwound (when the scheduler is being unlocked). */
            if get_pended_ticks!() == 0 {
                // vApplicationTickHook();
            } else {
                mtCOVERAGE_TEST_MARKER!();
            }
        }
    } else {
        set_pended_ticks!(get_pended_ticks!() + 1);

        /* The tick hook gets called at regular intervals, even if the
        scheduler is locked. */
        #[cfg(feature = "configUSE_TICK_HOOK")]
        vApplicationTickHook();

        #[cfg(feature = "configUSE_PREEMPTION")]
        {
            if get_yield_pending!() {
                switch_required = true;
            } else {
                mtCOVERAGE_TEST_MARKER!();
            }
        }
    }
    switch_required
}

#[cfg(any(
    feature = "INCLUDE_xTaskGetSchedulerState",
    feature = "configUSE_TIMERS"
))]
pub fn task_get_scheduler_state() -> SchedulerState {
    // These enums are defined at the top of this file.
    if !get_scheduler_running!() {
        SchedulerState::NotStarted
    } else {
        if get_scheduler_suspended!() == pdFALSE as UBaseType {
            SchedulerState::Running
        } else {
            SchedulerState::Suspended
        }
    }
}

/* Define away taskRESET_READY_PRIORITY() and portRESET_READY_PRIORITY() as
they are only required when a port optimised method of task selection is
being used. */
#[cfg(not(feature = "configUSE_PORT_OPTIMISED_TASK_SELECTION"))]
#[macro_export]
macro_rules! taskRESET_READY_PRIORITY {
    ($uxPriority: expr) => {};
}

/* uxTopReadyPriority holds the priority of the highest priority ready
state task. */
#[cfg(not(feature = "configUSE_PORT_OPTIMISED_TASK_SELECTION"))]
#[macro_export]
macro_rules! taskRECORD_READY_PRIORITY {
    ($uxPriority: expr) => {
        if $uxPriority > get_top_ready_priority!() {
            set_top_ready_priority!($uxPriority);
        }
    };
}
// Depress some warnings caused by our C bindings.
#![allow(non_upper_case_globals)]
#![allow(non_camel_case_types)]
#![allow(non_snake_case)]
#![allow(unused)]
#![feature(fnbox)]
#![feature(test)]
#![feature(weak_ptr_eq)]
#[macro_use]
extern crate lazy_static;
#[macro_use]
extern crate log;
extern crate simplelog;
extern crate test;

mod bindings; // This file is generated by bindgen and doesn't show up in the git repo.
pub mod config;
pub mod ffi;
pub mod list;
pub mod port;
pub mod projdefs;
pub mod task_control;
pub mod task_global;
mod trace;
// mod task_api;
pub mod kernel;
pub mod queue;
pub mod queue_api;
mod queue_h;
mod task_queue;
//mod mutex;
pub mod semaphore;
pub mod task_timemanager;

#[cfg(test)]
mod tests {
    use super::*;
    use test::Bencher;
    /*
    // Note! This test SHOULD FAIL, showing something like this:
    // test tests::test_vPortYield ... error: process didn't exit successfully: `/rust_freertos/target/debug/deps/rust_freertos-f3432ee83a2dce9a` (signal: 11, SIGSEGV: invalid memory reference)
    #[test]
    fn test_portYIELD() {
        portYIELD!()
    }
    */

    /*
    // Note! This test SHOULD FAIL as well.
    // BUT on my Mac it just doesn't stop running. Weird.
    use port;
    #[test]
    fn test_port_start_scheduler() {
        port::port_start_scheduler();
    }
    */

    use port;
    use task_control;
    #[bench]
    fn test_mutex(b: &mut Bencher) {
        use semaphore::Semaphore;
        use simplelog::*;

        let task0 = move || {
            let mut tmp = 1;
            for i in 1..11 {
                tmp = tmp * i;
            }
            kernel::task_end_scheduler();
        };

        let task1 = move || {
            let mut tmp = 1;
            for i in 1..11 {
                tmp = tmp * i;
            }
            kernel::task_end_scheduler();
        };

        let task2 = move || {
            let mut tmp = 1;
            for i in 1..11 {
                tmp = tmp * i;
            }
            kernel::task_end_scheduler();
        };

        let task3 = move || {
            let mut tmp = 1;
            for i in 1..11 {
                tmp = tmp * i;
            }
            kernel::task_end_scheduler();
        };

        let task4 = move || {
            let mut tmp = 1;
            for i in 1..11 {
                tmp = tmp * i;
            }
            kernel::task_end_scheduler();
        };

        let task5 = move || {
            let mut tmp = 1;
            for i in 1..11 {
                tmp = tmp * i;
            }
            kernel::task_end_scheduler();
        };

        let task6 = move || {
            let mut tmp = 1;
            for i in 1..11 {
                tmp = tmp * i;
            }
            kernel::task_end_scheduler();
        };

        let task7 = move || {
            let mut tmp = 1;
            for i in 1..11 {
                tmp = tmp * i;
            }
            kernel::task_end_scheduler();
        };


        b.iter(|| {let Task0 = task_control::TCB::new()
            .name("Task0")
            .priority(3)
            .initialise(task0);});
        b.iter(|| {let Task1 = task_control::TCB::new()
            .name("Task0")
            .priority(3)
            .initialise(task1);});
        b.iter(|| {let Task2 = task_control::TCB::new()
            .name("Task0")
            .priority(3)
            .initialise(task2);});
        b.iter(|| {let Task3 = task_control::TCB::new()
            .name("Task0")
            .priority(3)
            .initialise(task3);});
        b.iter(|| {let Task4 = task_control::TCB::new()
            .name("Task0")
            .priority(3)
            .initialise(task4);});
        b.iter(|| {let Task5 = task_control::TCB::new()
            .name("Task0")
            .priority(3)
            .initialise(task5);});
        b.iter(|| {let task6 = task_control::TCB::new()
            .name("Task0")
            .priority(3)
            .initialise(task6);});
        b.iter(|| {let Task7 = task_control::TCB::new()
            .name("Task0")
            .priority(3)
            .initialise(task7);});
        kernel::task_start_scheduler();
    }

    /*
    use std::sync::Arc;
    #[test]
    fn test_recursive_mutex() {
        use semaphore::Semaphore;
        use simplelog::*;

        let _ = TermLogger::init(LevelFilter::Trace, Config::default());
        let recursive_mutex = Semaphore::create_recursive_mutex();

        let mutex_holder = move || {
            for i in 1..11 {
                trace!("Call down_recursive");
                recursive_mutex.down_recursive(pdMS_TO_TICKS!(0));
                assert!(recursive_mutex.get_recursive_count() == i);
            }

            for j in 1..11 {
                recursive_mutex.up_recursive();
                assert!(recursive_mutex.get_recursive_count() == 10-j);
            }
            kernel::task_end_scheduler();
        };

        let recursive_mutex_holder = task_control::TCB::new()
                                .name("Recursive_mutex_holder")
                                .priority(3)
                                .initialise(mutex_holder);

        kernel::task_start_scheduler();
    }
    */

    /*
    #[test]
    fn test_mutex() {
        use semaphore::Semaphore;
        use simplelog::*;

        let _ = TermLogger::init(LevelFilter::Trace, Config::default());
        let mutex0 = Arc::new(Semaphore::new_mutex());
        let mutex1 = Arc::clone(&mutex0);

        let task0 = move || {
            task_timemanager::task_delay(pdMS_TO_TICKS!(1));
            loop {
                match mutex0.semaphore_down(pdMS_TO_TICKS!(0)) {
                    Ok(_) => {
                        for i in 1..11 {
                            trace!("Task0 owns the mutex! -- {}", i);
                        }
                        /*loop {
                            /*you can comment out this loop so that Task1 can successfully down the
                            counting_semaphore*/
                        }*/
                        match mutex0.semaphore_up() {
                            Ok(_) => {
                                trace!("Task0 dropped the mutex!");
                                kernel::task_end_scheduler();
                            }
                            Err(error) => {
                                trace!("mutex0 semaphore up triggers {}", error);
                            }
                        }
                    }
                    Err(error) => {
                        trace!("mutex0 semaphore take triggers {}", error);
                        task_timemanager::task_delay(pdMS_TO_TICKS!(1));
                        trace!("mutex0 delay in Err over!");
                    }
                }
            }
        };

        let task1 = move || {
            loop {
                match mutex1.semaphore_down(pdMS_TO_TICKS!(0)) {
                    Ok(_) => {
                        for i in 1..11 {
                            trace!("Task1 owns the mutex! -- {}", i);
                        }
                        task_timemanager::task_delay(pdMS_TO_TICKS!(1));
                        trace!("Task1's priority is {}", get_current_task_priority!());
                        /*loop {
                        }*/
                        match mutex1.semaphore_up() {
                            Ok(_) => {
                                trace!("Task1 dropped the mutex!");
                                task_timemanager::task_delay(pdMS_TO_TICKS!(1));
                                //     kernel::task_end_scheduler();
                            }
                            Err(error) => {
                                trace!("mutex1 semaphore up triggers {}", error);
                            }
                        }
                    }
                    Err(error) => {
                        trace!("mutex1 semaphore give triggers {}", error);
                    }
                }
            }
        };

        let Task0 = task_control::TCB::new()
            .name("Task0")
            .priority(4)
            .initialise(task0);

        let Task1 = task_control::TCB::new()
            .name("Task1")
            .priority(3)
            .initialise(task1);

        let Task12 = task_control::TCB::new()
            .name("Task2")
            .priority(3)
            .initialise(|| loop{});

        kernel::task_start_scheduler();
    }
*/
    /*
        #[test]
        fn test_counting_semaphore() {
            use simplelog::*;
            use semaphore::Semaphore;

            let _ = TermLogger::init(LevelFilter::Trace, Config::default());
            let cs0 = Arc::new(Semaphore::create_counting(2));
            let cs1 = Arc::clone(&cs0);
            let cs2 = Arc::clone(&cs0);

            let task_want_resources0 = move || {
                loop {
                    trace!("Enter Task0!");
                    match cs0.semaphore_down(pdMS_TO_TICKS!(10)) {
                        Ok(_) => {
                            for i in 1..11 {
                                trace!("cs0 owns the counting semaphore! -- {}", i);
                            }
                            // loop {
                                /*you can comment out this loop so that Task1 can successfully down the
                                counting_semaphore*/
                            // }
                            match cs0.semaphore_up() {
                                Ok(_) => {
                                    trace!("Task0 Finished!");
                                    break;
                                }
                                Err(error) => {
                                    trace!("cs0 semaphore give triggers {}", error);
                                }
                            }
                        },
                        Err(error) => {
                            trace!("cs0 semaphore take triggers {}", error);
                        },
                    }
                }
                loop {

                }
            };

            let task_want_resources1 = move || {
                loop {
                    trace!("Enter Task1!");
                    match cs1.semaphore_down(pdMS_TO_TICKS!(10)) {
                        Ok(_) => {
                            for i in 1..11 {
                                trace!("cs1 owns the counting semaphore! -- {}", i);
                            }
                            match cs1.semaphore_up() {
                                Ok(_) => {
                                    trace!("Test COUNTING SEMAPHORE COMPLETE!");
                                    kernel::task_end_scheduler();
                                    break;
                                }
                                Err(error) => {
                                    trace!("cs1 semaphore give triggers {}", error);
                                    kernel::task_end_scheduler();
                                }
                            }
                        },
                        Err(error) => {
                            trace!("cs1 semaphore take triggers {}", error);
                            kernel::task_end_scheduler();
                        },
                    }
                }
                loop {

                }
            };

            let task_want_resources2 = move || {
                loop {
                    trace!("Enter Task2!");
                    match cs2.semaphore_down(pdMS_TO_TICKS!(50)) {
                        Ok(_) => {
                            trace!("Task2 OK!");
                            for i in 1..11 {
                                trace!("cs2 owns the counting semaphore! -- {}", i);
                            }
                            loop {
                                /*you can comment out this loop so that Task1 can successfully down the
                                counting_semaphore*/
                            }
                            match cs2.semaphore_up() {
                                Ok(_) => {
                                    trace!("Task2 Finished!");
                                    break;
                                }
                                Err(error) => {
                                    trace!("cs2 semaphore give triggers {}", error);
                                }
                            }
                        },
                        Err(error) => {
                            trace!("cs2 semaphore take triggers {}", error);
                        },
                    }
                }
                loop {

                }
            };

            let _task0 = task_control::TCB::new()
                                    .name("Task0")
                                    .priority(3)
                                    .initialise(task_want_resources0);

            let _task1 = task_control::TCB::new()
                                    .name("Task1")
                                    .priority(3)
                                    .initialise(task_want_resources1);

            let _task2 = task_control::TCB::new()
                                    .name("Task2")
                                    .priority(3)
                                    .initialise(task_want_resources2);

            kernel::task_start_scheduler();

        }
    */

    /*
        #[test]
        fn test_queue() {
            use std::fs::File;
            use simplelog::*;
            use queue_api::Queue;

            // 两个任务共享所有权，所以需Arc包装。
            let q_recv = Arc::new(Queue::new(10));
            let q_sender = Arc::clone(&q_recv);
            let _ = TermLogger::init(LevelFilter::Trace, Config::default());

            // 发送数据的任务代码。
            let sender = move || {
                for i in 1..11 {
                    // send方法的参数包括要发送的数据和ticks_to_wait
                    q_sender.send(i, pdMS_TO_TICKS!(50)).unwrap();
                }
                loop {

                }
            };

            // 接收数据的任务代码。
            let receiver = move || {
                let mut sum = 0;
                loop {
                    // receive方法的参数只有ticks_to_wait
                    if let Ok(x) = q_recv.receive(pdMS_TO_TICKS!(10)) {
                        println!("{}", x);
                        sum += x;
                    } else {
                        trace!("receive END");
                        // 若等待30ms仍未收到数据，则认为发送结束。
                        assert_eq!(sum, 55);
                        kernel::task_end_scheduler();
                    }
                }
            };

            // 创建这两个任务。
            let _sender_task = task_control::TCB::new()
                                .name("Sender")
                                .priority(3)
                                .initialise(sender);

            let _receiver_task = task_control::TCB::new()
                                .name("Receiver")
                                .priority(3)
                                .initialise(receiver);

            kernel::task_start_scheduler();
        }
    */
}
use std::fmt;
use std::sync::{Arc, RwLock, Weak};

use crate::port::{portMAX_DELAY, TickType, UBaseType};
use crate::task_control::{TaskHandle, TCB};

impl fmt::Debug for ListItem {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "ListItem with value: {}", self.item_value)
    }
}

/// * Descrpition:
///  Definition of the only type of object that a list can contain.
///
/// * Implemented by: Fan Jinhao
///
pub struct ListItem {
    /* The value being listed.  In most cases this is used to sort the list in descending order. */
    item_value: TickType,
    /* Pointer to the next ListItem_t in the list. */
    next: WeakItemLink,
    /* Pointer to the previous ListItem_t in the list. */
    prev: WeakItemLink,
    /* Pointer to the object (normally a TCB) that contains the list item.
     * There is therefore a two way link between the object containing the list item
     * and the list item itself. */
    owner: Weak<RwLock<TCB>>,
    /* Pointer to the list in which this list item is placed (if any). */
    container: Weak<RwLock<List>>,
}

pub type ItemLink = Arc<RwLock<ListItem>>;
pub type WeakItemLink = Weak<RwLock<ListItem>>;
pub type WeakListLink = Weak<RwLock<List>>;
pub type ListLink = Arc<RwLock<List>>;

impl Default for ListItem {
    fn default() -> Self {
        ListItem {
            /* The list end value is the highest possible value in the list to
            ensure it remains at the end of the list. */
            item_value: portMAX_DELAY,
            next: Default::default(),
            owner: Default::default(),
            prev: Default::default(),
            container: Default::default(),
        }
    }
}

impl ListItem {
    pub fn item_value(mut self, item_value: TickType) -> Self {
        self.item_value = item_value;
        self
    }

    pub fn owner(mut self, owner: TaskHandle) -> Self {
        self.owner = owner.into();
        self
    }

    pub fn set_container(&mut self, container: &Arc<RwLock<List>>) {
        self.container = Arc::downgrade(container);
    }

    fn remove(&mut self, link: WeakItemLink) -> UBaseType {
        /* The list item knows which list it is in.  Obtain the list from the list
        item. */
        let list = self
            .container
            .upgrade()
            .unwrap_or_else(|| panic!("Container not set"));
        let ret_val = list.write().unwrap().remove_item(&self, link);
        set_list_item_next(&self.prev, Weak::clone(&self.next));
        set_list_item_prev(&self.next, Weak::clone(&self.prev));
        self.container = Weak::new();
        ret_val
    }
}

/// * Descrpition:
///  Definition of the type of queue used by the scheduler.
///
/// * Implemented by: Fan Jinhao
///
#[derive(Clone)]
pub struct List {
    number_of_items: UBaseType,
    /* Used to walk through the list.
     * Points to the last item returned by a call to listGET_OWNER_OF_NEXT_ENTRY (). */
    index: WeakItemLink,
    /* List item that contains the maximum possible item value meaning
     * it is always at the end of the list and is therefore used as a marker. */
    list_end: ItemLink,
}

impl Default for List {
    fn default() -> Self {
        /* The list structure contains a list item which is used to mark the
        end of the list.  To initialise the list the list end is inserted
        as the only list entry. */
        let list_end: ItemLink = Arc::new(RwLock::new(ListItem::default()));

        /* The list end next and previous pointers point to itself so we know
        when the list is empty. */
        list_end.write().unwrap().next = Arc::downgrade(&list_end);
        list_end.write().unwrap().prev = Arc::downgrade(&list_end);

        List {
            index: Arc::downgrade(&list_end),
            list_end: list_end,
            number_of_items: 0,
        }
    }
}

fn set_list_item_next(item: &WeakItemLink, next: WeakItemLink) {
    let owned_item = item
        .upgrade()
        .unwrap_or_else(|| panic!("List item is None"));
    (*owned_item.write().unwrap()).next = next;
}

fn set_list_item_prev(item: &WeakItemLink, prev: WeakItemLink) {
    let owned_item = item
        .upgrade()
        .unwrap_or_else(|| panic!("List item is None"));
    (*owned_item.write().unwrap()).prev = prev;
}

fn get_list_item_next(item: &WeakItemLink) -> WeakItemLink {
    let owned_item = item
        .upgrade()
        .unwrap_or_else(|| panic!("List item is None"));
    let next = Weak::clone(&(*owned_item.read().unwrap()).next);
    next
}

fn get_list_item_prev(item: &WeakItemLink) -> WeakItemLink {
    let owned_item = item
        .upgrade()
        .unwrap_or_else(|| panic!("List item is None"));
    let prev = Weak::clone(&(*owned_item.read().unwrap()).prev);
    prev
}

/// * Descrpition:
///  Access macro to retrieve the value of the list item.  The value can
///  represent anything - for example the priority of a task, or the time at
///  which a task should be unblocked.
///
/// * Implemented by: Fan Jinhao
///
/// # Arguments:
///
/// * Return:
///
pub fn get_list_item_value(item: &ItemLink) -> TickType {
    item.read().unwrap().item_value
}

/// * Descrpition:
///  Access macro to set the value of the list item.  In most cases the value is
///  used to sort the list in descending order.
///
/// * Implemented by: Fan Jinhao
///
/// # Arguments:
///
/// * Return:
///
pub fn set_list_item_value(item: &ItemLink, item_value: TickType) {
    item.write().unwrap().item_value = item_value;
}

fn get_weak_item_value(item: &WeakItemLink) -> TickType {
    let owned_item = item
        .upgrade()
        .unwrap_or_else(|| panic!("List item is None"));
    let value = owned_item.read().unwrap().item_value;
    value
}

fn set_weak_item_value(item: &WeakItemLink, item_value: TickType) {
    let owned_item = item
        .upgrade()
        .unwrap_or_else(|| panic!("List item is None"));
    owned_item.write().unwrap().item_value = item_value;
}

/// * Descrpition:
///  Return the list a list item is contained within (referenced from).
///
/// * Implemented by: Fan Jinhao
///
/// # Arguments:
///  `item` The list item being queried.
///
/// * Return:
///  A pointer to the List_t object that references the pxListItem
///
pub fn get_list_item_container(item: &ItemLink) -> Option<ListLink> {
    //let owned_item = item.upgrade().unwrap_or_else(|| panic!("List item is None"));
    let container = Weak::clone(&item.read().unwrap().container);
    container.upgrade()
}

/// * Descrpition:
///  Access macro to determine if a list contains any items.  The macro will
///  only have the value true if the list is empty.
///
/// * Implemented by: Fan Jinhao
///
/// # Arguments:
///
/// * Return:
///
pub fn list_is_empty(list: &ListLink) -> bool {
    list.read().unwrap().is_empty()
}

/// * Descrpition:
///  Access macro to return the number of items in the list.
///
/// * Implemented by: Fan Jinhao
///
/// # Arguments:
///
/// * Return:
///
pub fn current_list_length(list: &ListLink) -> UBaseType {
    list.read().unwrap().get_length()
}

/// * Descrpition:
///  Access function to get the owner of a list item.  The owner of a list item
///  is the object (usually a TCB) that contains the list item.
///
/// * Implemented by: Fan Jinhao
///
/// # Arguments:
///
/// * Return:
///
pub fn get_list_item_owner(item_link: &ItemLink) -> TaskHandle {
    let owner = Weak::clone(&item_link.read().unwrap().owner);
    owner.into()
}

/// * Descrpition:
///  Access function to set the owner of a list item.  The owner of a list item
///  is the object (usually a TCB) that contains the list item.
///
/// * Implemented by: Fan Jinhao
///
/// # Arguments:
///
/// * Return:
///
pub fn set_list_item_owner(item_link: &ItemLink, owner: TaskHandle) {
    item_link.write().unwrap().owner = owner.into()
}

/// * Descrpition:
///  Access function to obtain the owner of the next entry in a list.
///
///  The list member pxIndex is used to walk through a list.  Calling
///  listGET_OWNER_OF_NEXT_ENTRY increments pxIndex to the next item in the list
///  and returns that entry's pxOwner parameter.  Using multiple calls to this
///  function it is therefore possible to move through every item contained in
///  a list.
///
///  The pxOwner parameter of a list item is a pointer to the object that owns
///  the list item.  In the scheduler this is normally a task control block.
///  The pxOwner parameter effectively creates a two way link between the list
///  item and its owner.
///
/// * Implemented by: Fan Jinhao
///
/// # Arguments:
///  `list` The list from which the owner of the next item is to be
///  returned.
///
/// * Return:
/// The owner of next entry in list.
///
pub fn get_owner_of_next_entry(list: &ListLink) -> TaskHandle {
    let task = list.write().unwrap().get_owner_of_next_entry();
    task.into()
}

/// * Descrpition:
///  Access function to obtain the owner of the first entry in a list.  Lists
///  are normally sorted in ascending item value order.
///
///  This function returns the pxOwner member of the first item in the list.
///  The pxOwner parameter of a list item is a pointer to the object that owns
///  the list item.  In the scheduler this is normally a task control block.
///  The pxOwner parameter effectively creates a two way link between the list
///  item and its owner.
///
/// * Implemented by: Fan Jinhao
///
/// # Arguments:
///  `list` The list from which the owner of the head item is to be
///  returned.
///
/// * Return:
///
pub fn get_owner_of_head_entry(list: &ListLink) -> TaskHandle {
    let task = list.read().unwrap().get_owner_of_head_entry();
    task.into()
}

/// * Descrpition:
///  Check to see if a list item is within a list.  The list item maintains a
///  "container" pointer that points to the list it is in.  All this macro does
///  is check to see if the container and the list match.
///
/// * Implemented by: Fan Jinhao
///
/// # Arguments:
///
/// * Return:
///
pub fn is_contained_within(list: &ListLink, item_link: &ItemLink) -> bool {
    match get_list_item_container(&item_link) {
        Some(container) => Arc::ptr_eq(list, &container),
        None => false,
    }
}

/// * Descrpition:
///  Insert a list item into a list.  The item will be inserted into the list in
///  a position determined by its item value (descending item value order).
///
/// * Implemented by: Fan Jinhao
///
/// # Arguments:
///  `list` The list into which the item is to be inserted.
///
///  `item_link` The item that is to be placed in the list.
///
/// * Return:
///
pub fn list_insert(list: &ListLink, item_link: ItemLink) {
    /* Remember which list the item is in.  This allows fast removal of the
    item later. */
    item_link.write().unwrap().set_container(&list);
    println!("Set conatiner");
    list.write().unwrap().insert(Arc::downgrade(&item_link))
}

/// * Descrpition:
///  Insert a list item into a list.  The item will be inserted in a position
///  such that it will be the last item within the list returned by multiple
///  calls to listGET_OWNER_OF_NEXT_ENTRY.
///
///  The list member pxIndex is used to walk through a list.  Calling
///  listGET_OWNER_OF_NEXT_ENTRY increments pxIndex to the next item in the list.
///  Placing an item in a list using vListInsertEnd effectively places the item
///  in the list position pointed to by pxIndex.  This means that every other
///  item within the list will be returned by listGET_OWNER_OF_NEXT_ENTRY before
///  the pxIndex parameter again points to the item being inserted.
///
/// * Implemented by: Fan Jinhao
///
/// # Arguments:
///  `list` The list into which the item is to be inserted.
///
///  `item_link` The list item to be inserted into the list.
///
/// * Return:
///
pub fn list_insert_end(list: &ListLink, item_link: ItemLink) {
    /* Insert a new list item into pxList, but rather than sort the list,
    makes the new list item the last item to be removed by a call to
    listGET_OWNER_OF_NEXT_ENTRY(). */

    /* Remember which list the item is in. */
    item_link.write().unwrap().set_container(&list);

    list.write().unwrap().insert_end(Arc::downgrade(&item_link))
}

/// * Descrpition:
///  Remove an item from a list.  The list item has a pointer to the list that
///  it is in, so only the list item need be passed into the function.
///
/// * Implemented by: Fan Jinhao
///
/// # Arguments:
///  `item_link` The item to be removed.  The item will remove itself from
///  the list pointed to by it's pxContainer parameter.
///
/// * Return:
///  The number of items that remain in the list after the list item has
///  been removed.
///
pub fn list_remove(item_link: ItemLink) -> UBaseType {
    item_link
        .write()
        .unwrap()
        .remove(Arc::downgrade(&item_link))
}

impl List {
    fn insert(&mut self, item_link: WeakItemLink) {
        println!("in");
        let value_of_insertion = get_weak_item_value(&item_link);
        /* Insert the new list item into the list, sorted in xItemValue order.

        If the list already contains a list item with the same item value then the
        new list item should be placed after it.  This ensures that TCB's which are
        stored in ready lists (all of which have the same xItemValue value) get a
        share of the CPU.  However, if the xItemValue is the same as the back marker
        the iteration loop below will not end.  Therefore the value is checked
        first, and the algorithm slightly modified if necessary. */
        let item_to_insert = if value_of_insertion == portMAX_DELAY {
            get_list_item_prev(&Arc::downgrade(&self.list_end))
        } else {
            /* *** NOTE ***********************************************************
              If you find your application is crashing here then likely causes are
              listed below.  In addition see http://www.freertos.org/FAQHelp.html for
              more tips, and ensure configASSERT() is defined!
              http://www.freertos.org/a00110.html#configASSERT

              1) Stack overflow -
              see http://www.freertos.org/Stacks-and-stack-overflow-checking.html
              2) Incorrect interrupt priority assignment, especially on Cortex-M
              parts where numerically high priority values denote low actual
              interrupt priorities, which can seem counter intuitive.  See
              http://www.freertos.org/RTOS-Cortex-M3-M4.html and the definition
              of configMAX_SYSCALL_INTERRUPT_PRIORITY on
              http://www.freertos.org/a00110.html
              3) Calling an API function from within a critical section or when
              the scheduler is suspended, or calling an API function that does
              not end in "FromISR" from an interrupt.
              4) Using a queue or semaphore before it has been initialised or
              before the scheduler has been started (are interrupts firing
              before vTaskStartScheduler() has been called?).
            **********************************************************************/
            let mut iterator = Arc::downgrade(&self.list_end);
            loop {
                /* There is nothing to do here, just iterating to the wanted
                insertion position. */
                let next = get_list_item_next(&iterator);
                if get_weak_item_value(&next) > value_of_insertion {
                    break iterator;
                }
                iterator = next;
            }
        };

        let prev = Weak::clone(&item_to_insert);
        let next = get_list_item_next(&item_to_insert);

        set_list_item_next(&item_link, Weak::clone(&next));
        set_list_item_prev(&item_link, Weak::clone(&prev));
        set_list_item_next(&prev, Weak::clone(&item_link));
        set_list_item_prev(&next, Weak::clone(&item_link));

        self.number_of_items += 1;
    }

    fn insert_end(&mut self, item_link: WeakItemLink) {
        let prev = get_list_item_prev(&self.index);
        let next = Weak::clone(&self.index);
        set_list_item_next(&item_link, Weak::clone(&next));
        set_list_item_prev(&item_link, Weak::clone(&prev));
        set_list_item_next(&prev, Weak::clone(&item_link));
        set_list_item_prev(&next, Weak::clone(&item_link));

        self.number_of_items += 1;
    }

    fn remove_item(&mut self, item: &ListItem, link: WeakItemLink) -> UBaseType {
        // TODO: Find a more effiecient
        if Weak::ptr_eq(&link, &self.index) {
            self.index = Weak::clone(&item.prev);
        }

        self.number_of_items -= 1;

        self.number_of_items
    }

    fn is_empty(&self) -> bool {
        self.number_of_items == 0
    }

    fn get_length(&self) -> UBaseType {
        self.number_of_items
    }

    fn increment_index(&mut self) {
        self.index = get_list_item_next(&self.index);
        if Weak::ptr_eq(&self.index, &Arc::downgrade(&self.list_end)) {
            self.index = get_list_item_next(&self.index);
        }
    }

    fn get_owner_of_next_entry(&mut self) -> Weak<RwLock<TCB>> {
        self.increment_index();
        let owned_index = self
            .index
            .upgrade()
            .unwrap_or_else(|| panic!("List item is None"));
        let owner = Weak::clone(&owned_index.read().unwrap().owner);
        owner
    }

    fn get_owner_of_head_entry(&self) -> Weak<RwLock<TCB>> {
        let list_end = get_list_item_next(&Arc::downgrade(&self.list_end));
        let owned_index = list_end
            .upgrade()
            .unwrap_or_else(|| panic!("List item is None"));
        let owner = Weak::clone(&owned_index.read().unwrap().owner);
        owner
    }
}

/*
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_all() {
        let list = Arc::new(RwLock::new(List::default()));
        println!("List");

        let t1 = TCB::new(3, &list).init();
        let t2 = TCB::new(2, &list).init();

        let x = get_owner_of_head_entry(&list);
        println!("{:?}, should be 3", x);

        let t3 = TCB::new(5, &list).init();

        let x = get_owner_of_next_entry(&list);
        println!("{:?}, should be 3", x);

        let t4 = TCB::new(1, &list).init();

        let x = get_owner_of_next_entry(&list);
        println!("{:?}, should be 2", x);

        let t5 = TCB::new(0, &list).init();

        let x = get_owner_of_next_entry(&list);
        println!("{:?}, should be 5", x);

        let x = list_remove(Arc::clone(&t2.0.read().unwrap().item));
        assert_eq!(x, 4);

        let x = get_owner_of_head_entry(&list);
        println!("{:?}, should be 1", x);

        let item = Arc::clone(&t1.0.read().unwrap().item);
        assert!(is_contained_within(&list, Arc::clone(&item)));
        let x = list_remove(Arc::clone(&item));
        // assert!(!is_contained_within(&list, Arc::clone(&item)));
        assert_eq!(x, 3);

        let x = get_owner_of_next_entry(&list);
        println!("{:?}, should be 1", x);

        let x = get_owner_of_next_entry(&list);
        println!("{:?}, should be 0", x);

        let x = get_owner_of_next_entry(&list);
        println!("{:?}, should be 5", x);

        let x = get_owner_of_next_entry(&list);
        println!("{:?}, should be 1", x);
    }
}
*/
// port.c - The wrapper of portable functions written in C.
// This file is created by Fan Jinhao.
use crate::bindings::*;
use crate::projdefs::FreeRtosError;

// NOTE! These type aliases may vary across different platforms.
// TODO: Find a better way to define these types.
pub type StackType = usize;
pub type BaseType = i64;
pub type UBaseType = u64;
pub type TickType = u32;
pub type CVoidPointer = *mut std::os::raw::c_void;

#[cfg(target_arch = "x86_64")]
pub const portBYTE_ALIGNMENT_MASK: UBaseType = 8;
#[cfg(not(target_arch = "x86_64"))]
pub const portBYTE_ALIGNMENT_MASK: UBaseType = 4;

#[cfg(feature = "configUSE_16_BIT_TICKS")]
pub const portMAX_DELAY: TickType = 0xffff;
#[cfg(not(feature = "configUSE_16_BIT_TICKS"))]
pub const portMAX_DELAY: TickType = 0xffffffff;

/* -------------------- Macros starting with "port_" ----------------- */
#[macro_export]
macro_rules! portYIELD {
    () => {
        unsafe { crate::bindings::vPortYield() }
    };
}

// TODO: Is it appropriate to place this definition here?
#[macro_export]
macro_rules! portYIELD_WITHIN_API {
    () => {
        portYIELD!()
    };
}

#[macro_export]
macro_rules! portEND_SWITCHING_ISR {
    ($xSwitchRequired: expr) => {
        if $xSwitchRequired {
            unsafe {
                crate::bindings::vPortYieldFromISR();
            }
        }
    };
}

#[macro_export]
macro_rules! portYIELD_FROM_ISR {
    ($xSwitchRequired: expr) => {
        unsafe { portEND_SWITCHING_ISR($xSwitchRequired) }
    };
}

#[macro_export]
macro_rules! portSET_INTERRUPT_MASK_FROM_ISR {
    () => {
        unsafe { (crate::bindings::xPortSetInterruptMask() as BaseType) }
    };
}

#[macro_export]
macro_rules! portCLEAR_INTERRUPT_MASK_FROM_ISR {
    ($xMask: expr) => {
        unsafe { crate::bindings::vPortClearInterruptMask($xMask as BaseType) }
    };
}

#[macro_export]
macro_rules! portSET_INTERRUPT_MASK {
    () => {
        unsafe { crate::bindings::vPortDisableInterrupts() }
    };
}

#[macro_export]
macro_rules! portCLEAR_INTERRUPT_MASK {
    () => {
        unsafe { crate::bindings::vPortEnableInterrupts() }
    };
}

#[macro_export]
macro_rules! portDISABLE_INTERRUPTS {
    () => {
        unsafe { portSET_INTERRUPT_MASK!() }
    };
}

#[macro_export]
macro_rules! portENABLE_INTERRUPTS {
    () => {
        unsafe { portCLEAR_INTERRUPT_MASK() }
    };
}

#[macro_export]
macro_rules! portENTER_CRITICAL {
    () => {
        unsafe { crate::bindings::vPortEnterCritical() }
    };
}

#[macro_export]
macro_rules! portEXIT_CRITICAL {
    () => {
        unsafe { crate::bindings::vPortExitCritical() }
    };
}

// TODO: TASK_FUNCTION and TASK_FUNCTION_PROTO may be defined as a macro.
// They were not defined because we haven't decided the prototype of a task function.

#[macro_export]
macro_rules! portNOP {
    () => {
        // This is an empty function.
    };
}

#[macro_export]
macro_rules! traceTASK_DELETE {
    ($pxTaskToDelete: expr) => {
        unsafe {
            // TODO: Add a trace!()
            bindings::vPortForciblyEndThread(std::sync::Arc::into_raw($pxTaskToDelete) as *mut _)
        }
    };
}

#[macro_export]
macro_rules! traceTASK_CREATE {
    ($pxTaskHandle: expr) => {
        unsafe {
            trace!("Task creation accomplished.");
            bindings::vPortAddTaskHandle($pxTaskHandle.as_raw())
        }
    };
}

#[macro_export]
macro_rules! portCONFIGURE_TIMER_FOR_RUN_TIME_STATS {
    () => {
        unsafe { crate::bindings::vPortFindTicksPerSecond() }
    };
}

#[macro_export]
macro_rules! portGET_RUN_TIME_COUNTER_VALUE {
    () => {
        unsafe { crate::bindings::ulPortGetTimerValue() }
    };
}

#[macro_export]
macro_rules! portTICK_PERIOS_MS {
    () => {
        1000 as TickType / config::configTICK_RATE_HZ!()
    };
}

// This macro was not implemented by port.c, so it was left blank.
// You can modify it yourself.
#[macro_export]
macro_rules! portCLEAN_UP_TCB {
    ($pxTCB: expr) => {
        $pxTCB
    };
}

// This macro was not implemented by port.c, so it was left blank.
// You can modify it yourself.
#[macro_export]
macro_rules! portPRE_TASK_DELETE_HOOK {
    ($pvTaskToDelete:expr, $pxYieldPending: expr) => {};
}

// This macro was not implemented by port.c, so it was left blank.
// You can modify it yourself.
#[macro_export]
macro_rules! portSETUP_TCB {
    ($pxTCB:expr) => {
        $pxTCB
    };
}

// This macro was not implemented by port.c, so it was left blank.
// You can modify it yourself.
#[macro_export]
macro_rules! portSUPPRESS_TICKS_AND_SLEEP {
    ($xExpectedIdleTime:expr) => {};
}

// This macro was not implemented by port.c, so it was left blank.
// You can modify it yourself.
#[macro_export]
macro_rules! portTASK_USES_FLOATING_POINT {
    () => {};
}

// This macro was not implemented by port.c, so it was left blank.
// You can modify it yourself.
#[macro_export]
macro_rules! portASSERT_IF_INTERRUPT_PRIORITY_INVALID {
    () => {};
}

// This macro was not implemented by port.c, so it was left blank.
// You can modify it yourself.
#[macro_export]
macro_rules! portASSERT_IF_IN_ISR {
    () => {};
}

#[macro_export]
macro_rules! portRESET_READY_PRIORITY {
    ($uxPriority: expr, $uxTopReadyPriority: expr) => {
        // This macro does nothing.
    };
}

/*------------------- Functions starting with "Port_" ----------------- */

// NOTE: I made some changes to the following function names!

/*
 * Map to the memory management routines required for the port.
 */
pub fn port_malloc(size: usize) -> Result<CVoidPointer, FreeRtosError> {
    unsafe {
        let ret_ptr = pvPortMalloc(size);
        if ret_ptr.is_null() {
            error!("Malloc returned null.");
            Err(FreeRtosError::OutOfMemory)
        } else {
            Ok(ret_ptr)
        }
    }
}

pub fn port_free(pv: *mut ::std::os::raw::c_void) {
    unsafe { vPortFree(pv) }
}

/* NOTE: vPortInitialiseBlocks() was declared but not implemented.

    pub fn port_initialize_blocks() {
        unsafe {
            vPortInitialiseBlocks()
        }
    }

*/

/* NOTE: xPortGetFreeHeapSize() was declared but not implemented

    pub fn port_get_free_heap_size() -> usize{
        unsafe {
            xPortGetFreeHeapSize()
        }
    }

*/

/* NOTE: xPortGetMinimumEverFreeHeapSize() was declared but not implemented

    pub fn port_get_minimum_ever_free_heap_size() -> usize {
        unsafe {
            xPortGetMinimumEverFreeHeapSize()
        }
    }

*/

/*
 * Setup the hardware ready for the scheduler to take control.  This generally
 * sets up a tick interrupt and sets timers for the correct tick frequency.
 */
pub fn port_start_scheduler() -> BaseType {
    unsafe { xPortStartScheduler() }
}

/*
 * Undo any hardware/ISR setup that was performed by xPortStartScheduler() so
 * the hardware is left in its original condition after the scheduler stops
 * executing.
 */
pub fn port_end_scheduler() {
    unsafe { vPortEndScheduler() }
}

/*
 * Setup the stack of a new task so it is ready to be placed under the
 * scheduler control.  The registers have to be placed on the stack in
 * the order that the port expects to find them.
 *
 */
pub fn port_initialise_stack(
    pxTopOfStack: *mut StackType,
    pxCode: TaskFunction_t,
    pvParameters: *mut ::std::os::raw::c_void,
) -> Result<*mut StackType, FreeRtosError> {
    let ret_val = unsafe { pxPortInitialiseStack(pxTopOfStack, pxCode, pvParameters) };
    if ret_val.is_null() {
        error!("Port failed to initialise task stack!");
        Err(FreeRtosError::PortError)
    } else {
        Ok(ret_val)
    }
}
// projdefs.rs - Basic (maybe useless) constant definitions.
use crate::port::BaseType;

pub const pdTRUE: BaseType = 1;
pub const pdFALSE: BaseType = 0;

pub const pdPASS: BaseType = pdTRUE;
pub const pdFAIL: BaseType = pdFALSE;

#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub enum FreeRtosError {
    OutOfMemory,
    Timeout,
    TaskNotFound,
    ProcessorHasShutDown,
    DeadLocked,
    PortError,
}

#[macro_export]
macro_rules! pdMS_TO_TICKS {
    ($xTimeInMs:expr) => {
        (($xTimeInMs as crate::port::TickType * configTICK_RATE_HZ!())
            / 1000 as crate::port::TickType) as crate::port::TickType
    };
}
// queue_api.rs, queue APIs
// This file is created by Ning Yuting.
// To solve the issue of mutability of queue.

use crate::port::*;
use crate::queue::*;
use crate::queue_h::*;
use std::cell::UnsafeCell;

/// * Description:
///
/// Implemente interior mutability for queue so that queue can be shared among threads as immutable
/// inference.
/// It is safe to use lots of unsafe codes here because we implemente synchronous blocking for
/// queue.
///
/// * Implemented by:Ning Yuting
pub struct Queue<T>(UnsafeCell<QueueDefinition<T>>)
where
    T: Default + Clone;

// send, sync is used for sharing queue among threads
unsafe impl<T: Default + Clone> Send for Queue<T> {}
unsafe impl<T: Default + Clone> Sync for Queue<T> {}

impl<T> Queue<T>
where
    T: Default + Clone,
{
    /*some APIs in queue.h */

    /// # Description:
    /// Create a new queue.
    /// 
    /// * Implemented by:Ning Yuting
    /// * C implementation:queue.h 186
    ///
    /// # Arguments:
    /// * `length` - The maximum number of items that the queue can contain.
    ///
    /// # Return:
    /// The created queue.
    pub fn new(length: UBaseType) -> Self {
        Queue(UnsafeCell::new(QueueDefinition::new(
            length,
            QueueType::Base,
        )))
    }

    /// # Description
    /// Post an item to the front of a queue.
    /// 
    /// * Implemented by:Ning Yuting
    /// * C implementation: queue.h 521
    ///
    /// # Argument
    /// * `pvItemToQueue` - the item that is to be placed on the queue.
    /// * `xTicksToWait` - The maximum amount of time the task should block waiting for space to 
    /// become available on the queue, should it already be full.
    ///
    /// # Return
    /// Ok() if the item was successfully posted, otherwise errQUEUE_FULL.
    pub fn send(&self, pvItemToQueue: T, xTicksToWait: TickType) -> Result<(), QueueError> {
        unsafe {
            let inner = self.0.get();
            (*inner).queue_generic_send(pvItemToQueue, xTicksToWait, queueSEND_TO_BACK)
        }
    }

    /// # Description
    /// Post an item to the front of a queue.
    /// 
    /// * Implemented by:Ning Yuting
    /// * C implementation:queue.h 355
    ///
    /// # Argument
    /// * `pvItemToQueue` - the item that is to be placed on the queue.
    /// * `xTicksToWait` - The maximum amount of time the task should block waiting for space to
    /// become available on the queue, should it already be full.
    /// 
    /// # Return
    /// Ok() if the item was successfully posted, otherwise errQUEUE_FULL.
    pub fn send_to_front(
        &self,
        pvItemToQueue: T,
        xTicksToWait: TickType,
    ) -> Result<(), QueueError> {
        unsafe {
            let inner = self.0.get();
            (*inner).queue_generic_send(pvItemToQueue, xTicksToWait, queueSEND_TO_FRONT)
        }
    }

    /// # Description
    /// Post an item to the back of a queue.
    ///
    /// * Implemented by:Ning Yuting
    /// * C implementation:queue.h 437
    ///
    /// # Argument
    /// * `pvItemToQueue` - the item that is to be placed on the queue.
    /// * `xTicksToWait` - The maximum amount of time the task should block waiting for space to 
    /// become available on the queue, should it already be full.
    /// 
    /// # Return
    /// Ok() if the item was successfully posted, otherwise errQUEUE_FULL.
    pub fn send_to_back(&self, pvItemToQueue: T, xTicksToWait: TickType) -> Result<(), QueueError> {
        unsafe {
            let inner = self.0.get();
            (*inner).queue_generic_send(pvItemToQueue, xTicksToWait, queueSEND_TO_BACK)
        }
    }

    /// # Description
    /// Only for use with queues that have a length of one - so the queue is either empty or full.
    /// Post an item on a queue.If the queue is already full then overwrite the value held in the queue.
    ///
    /// * Implemented by:Ning Yuting
    /// * C implementation:queue.h 604
    ///
    /// # Argument
    /// * `pvItemToQueue` - the item that is to be place on the queue.
    ///
    /// # Return
    /// Ok() is the only value that can be returned because queue_overwrite will write to the
    /// queue even when the queue is already full.
    pub fn overwrite(&self, pvItemToQueue: T) -> Result<(), QueueError> {
        unsafe {
            let inner = self.0.get();
            (*inner).queue_generic_send(pvItemToQueue, 0, queueOVERWRITE)
        }
    }

    /// # Description
    /// Post an item to the front of a queue. It is safe to use this function from within an interrupt service routine.
    ///
    /// * Implemented by:Ning Yuting
    /// * C implementation:queue.h 1129
    ///
    /// # Argument
    /// * `pvItemToQueue - the item that is to be placed on the queue.
    ///
    /// # Return
    /// * `Result` -Ok() if the data was successfully sent to the queue, otherwise errQUEUE_FULL.
    /// * `bool` - pxHigherPriorityTaskWoken is changed to be a return value. it is true if sending to the
    /// queue caused a task to unblock,otherwise it is false.
    pub fn send_to_front_from_isr(&self, pvItemToQueue: T) -> (Result<(), QueueError>, bool) {
        unsafe {
            let inner = self.0.get();
            (*inner).queue_generic_send_from_isr(pvItemToQueue, queueSEND_TO_FRONT)
        }
    }

    /// # Description
    /// Post an item to the back of a queue. It is safe to use this function from within an interrupt service routi    ne.
    ///
    /// * Implemented by:Ning Yuting
    /// * C implementation:queue.h 1200
    ///
    /// # Argument
    /// * `pvItemToQueue - the item that is to be placed on the queue.
    /// # Return
    /// * `Result` -Ok() if the data was successfully sent to the queue, otherwise errQUEUE_FULL.
    /// * `bool` - pxHigherPriorityTaskWoken is changed to be a return value. it is true if sending to the
    /// queue caused a task to unblock,otherwise it is false.
    pub fn send_to_back_from_isr(&self, pvItemToQueue: T) -> (Result<(), QueueError>, bool) {
        unsafe {
            let inner = self.0.get();
            (*inner).queue_generic_send_from_isr(pvItemToQueue, queueSEND_TO_BACK)
        }
    }

    /// # Description
    /// A version of xQueueOverwrite() that can be used in an interrupt service routine (ISR).
    ///
    /// * Implemented by:Ning Yuting
    /// * C implementation:queue.h 1287
    ///
    /// # Argument
    /// * `pvItemToQueue - the item that is to be placed on the queue.
    /// # Return
    /// * `Result` -Ok().
    /// * `bool` - pxHigherPriorityTaskWoken is changed to be a return value. it is true if sending to the
    /// queue caused a task to unblock,otherwise it is false.
    pub fn overwrite_from_isr(&self, pvItemToQueue: T) -> (Result<(), QueueError>, bool) {
        unsafe {
            let inner = self.0.get();
            (*inner).queue_generic_send_from_isr(pvItemToQueue, queueOVERWRITE)
        }
    }

    /// # Description
    /// Receive an item from a queue.
    /// The item is received by copy and is returned by Ok(T);
    /// Successfully received items are removed from the queue.
    ///
    /// * Implemented by:Ning Yuting
    /// * C implementation:queue.h 913
    ///
    /// # Argument
    /// * `xTicksToWait` - The maximum amount of time the task should block 
    /// waiting for an item to receive should the queue be empty at the time
    /// of the call.It will return immediately if xTicksToWait is zero and the queue is empty.
    /// 
    /// # Return
    /// Ok(T) if an item was successfully received from the queue, otherwise QueueError::QueueEmpty.
    pub fn receive(&self, xTicksToWait: TickType) -> Result<T, QueueError> {
        unsafe {
            let inner = self.0.get();
            (*inner).queue_generic_receive(xTicksToWait, false)
        }
    }

    /// # Description
    /// Receive an item from a queue without removing the item from the queue.
    /// The item is received by copy and is returned by Ok(T);
    /// Successfully received items remain on the queue.
    ///
    /// * Implemented by:Ning Yuting
    /// * C implementation:queue.h 787
    ///
    /// # Argument
    /// * `xTicksToWait` - The maximum amount of time the task should block 
    /// waiting for an item to receive should the queue be empty at the time of the call.
    /// 
    /// # Return
    /// Ok(T) if an item was successfully received from the queue, otherwise
    /// QueueError::QueueEmpty.
    pub fn peek(&self, xTicksToWait: TickType) -> Result<T, QueueError> {
        unsafe {
            let inner = self.0.get();
            (*inner).queue_generic_receive(xTicksToWait, true)
        }
    }
}
use crate::port::*;
use std::fmt;

#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub enum QueueError {
    QueueSendTimeout,
    QueueReceiveTimeout,
    MutexTimeout,
    QueueFull,
    QueueEmpty,
}

impl fmt::Display for QueueError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            QueueError::QueueSendTimeout => write!(f, "QueueSendTimeOut"),
            QueueError::QueueReceiveTimeout => write!(f, "QueueReceiveTimeOut"),
            QueueError::MutexTimeout => write!(f, "MutexSendTimeOut"),
            QueueError::QueueFull => write!(f, "QueueFull"),
            QueueError::QueueEmpty => write!(f, "QueueEmpty"),
        }
    }
}

pub const queueSEND_TO_BACK: BaseType = 0;
pub const queueSEND_TO_FRONT: BaseType = 1;
pub const queueOVERWRITE: BaseType = 2;

pub const semGIVE_BLOCK_TIME: TickType = 0;

#[derive(PartialEq)]
pub enum QueueType {
    Base,
    Set,
    Mutex,
    CountingSemaphore,
    BinarySemaphore,
    RecursiveMutex,
}
impl Default for QueueType {
    fn default() -> Self {
        QueueType::Base
    }
}
use std::collections::VecDeque;
use crate::port::*;
use crate::list::*;
use crate::queue_h::*;
use crate::*;
use crate::task_queue::*;

pub const queueQUEUE_IS_MUTEX: UBaseType = 0;
pub const queueUNLOCKED: i8 = -1;
pub const queueLOCKED_UNMODIFIED: i8 = 0;
pub const queueSEMAPHORE_QUEUE_ITEM_LENGTH: UBaseType = 0;
pub const queueMUTEX_GIVE_BLOCK_TIME: TickType = 0;

#[derive(Default)]
pub struct QueueDefinition<T>
where
    T: Default + Clone,
{
    pcQueue: VecDeque<T>,

    pcHead: UBaseType,
    pcTail: UBaseType,
    pcWriteTo: UBaseType,

    /*pcReadFrom & uxRecuriveCallCount*/
    QueueUnion: UBaseType,

    xTasksWaitingToSend: ListLink,
    xTasksWaitingToReceive: ListLink,

    uxMessagesWaiting: UBaseType,
    uxLength: UBaseType,
    cRxLock: i8,
    cTxLock: i8,

    #[cfg(all(
        feature = "configSUPPORT_STATIC_ALLOCATION",
        feature = "configSUPPORT_DYNAMIC_ALLOCATION"
    ))]
    ucStaticallyAllocated: u8,

    #[cfg(feature = "configUSE_QUEUE_SETS")]
    pxQueueSetContainer: Option<Box<QueueDefinition>>,

    #[cfg(feature = "configUSE_TRACE_FACILITY")]
    uxQueueNumber: UBaseType,
    //#[cfg(feature = "configUSE_TRACE_FACILITY")]
    ucQueueType: QueueType,
}

impl<T> QueueDefinition<T>
where
    T: Default + Clone,
{
    /// # Description
    /// Create a new queue.
    ///
    /// * Implemented by:Lei Siqi
    /// * Modifiled by: Ning Yuting
    /// * C implementation:queue.c 384-429
    /// # Argument
    /// `uxQueueLength` - the length of the queue
    /// `ucQueueType` - the type of the queue
    ///
    /// # Return
    /// The created queue.
    #[cfg(feature = "configSUPPORT_DYNAMIC_ALLOCATION")]
    pub fn queue_generic_create(uxQueueLength: UBaseType, ucQueueType: QueueType) -> Self {
        let mut queue: QueueDefinition<T> = Default::default();
        queue.pcQueue = VecDeque::with_capacity(uxQueueLength as usize);
        queue.initialise_new_queue(uxQueueLength, ucQueueType);
        queue
    }

    /// # Description
    /// *
    /// * Implemented by:Lei Siqi
    /// # Argument
    ///
    /// # Return
    ///
    pub fn initialise_new_queue(&mut self, uxQueueLength: UBaseType, ucQueueType: QueueType) {
        self.pcHead = 0;
        self.uxLength = uxQueueLength;
        self.queue_generic_reset(true);

        self.ucQueueType = ucQueueType;

        {
            #![cfg(feature = "configUSE_QUEUE_SETS")]
            self.pxQueueSetContainer = None;
        }

        traceQUEUE_CREATE!(&self);
    }

    /// # Description
    /// Reset the queue.
    ///
    /// * Implemented by:Ning Yuting
    /// * C implementation:queue.c 279-329
    /// 
    /// # Argument
    /// * `xNewQueue` - whether the queue is a new queue
    /// 
    /// # Return
    /// `Result<(),QueueError>` - Ok() if the queue was successfully reseted.
    pub fn queue_generic_reset(&mut self, xNewQueue: bool) -> Result<(), QueueError> {
        //xNewQueue源码中为BaseType，改为bool
        //返回值原为BaseType，改为result
        taskENTER_CRITICAL!();
        {
            //初始化队列相关成员变量
            self.pcTail = self.pcHead + self.uxLength;
            self.uxMessagesWaiting = 0 as UBaseType;
            self.pcWriteTo = self.pcHead;
            self.QueueUnion = self.pcHead + self.uxLength - (1 as UBaseType); //QueueUnion represents pcReadFrom
            self.cRxLock = queueUNLOCKED;
            self.cTxLock = queueUNLOCKED;
            self.pcQueue.clear(); //初始化空队列
            if xNewQueue == false {
                if list::list_is_empty(&self.xTasksWaitingToSend) == false {
                    if task_queue::task_remove_from_event_list(&self.xTasksWaitingToSend) != false {
                        queueYIELD_IF_USING_PREEMPTION!();
                    } else {
                        mtCOVERAGE_TEST_MARKER!();
                    }
                } else {
                    mtCOVERAGE_TEST_MARKER!();
                }
            } else {
                self.xTasksWaitingToSend = Default::default();
                self.xTasksWaitingToReceive = Default::default();
            }
        }
        taskEXIT_CRITICAL!();
        Ok(())
    }

    /// # Description
    /// Post a item to the queue.
    ///
    /// * Implemented by:Lei Siqi
    /// * Modifiled by: Ning Yuting
    /// * C implementation: 723-918
    ///
    /// # Argument
    /// `pvItemToQueue` - the item that is to be placed to the queue.
    /// `xCopyPosition` - the position that the item is to be placed to.
    ///
    /// # Return
    /// Ok() if the item is successfully posted, otherwise Err(QueueError::QueueEmpty).
    pub fn queue_generic_send(
        &mut self,
        pvItemToQueue: T,
        xTicksToWait: TickType,
        xCopyPosition: BaseType,
    ) -> Result<(), QueueError> {
        let mut xEntryTimeSet: bool = false;
        let mut xTimeOut: time_out = Default::default();
        let mut xTicksToWait = xTicksToWait;

        assert!(!((xCopyPosition == queueOVERWRITE) && self.uxLength == 1));

        #[cfg(all(feature = "xTaskGetSchedulerState", feature = "configUSE_TIMERS"))]
        assert!(
            !((kernel::task_get_scheduler_state() == SchedulerState::Suspended)
                && (xTicksToWait != 0))
        );
        trace!("Enter function queue_generic_send! TicksToWait: {}, uxMessageWaiting: {}, xCopyPosition: {}", xTicksToWait ,self.uxMessagesWaiting, xCopyPosition);
        /* This function relaxes the coding standard somewhat to allow return
        statements within the function itself.  This is done in the interest
        of execution time efficiency. */
        loop {
            taskENTER_CRITICAL!();
            {
                /* Is there room on the queue now?  The running task must be the
                highest priority task wanting to access the queue.  If the head item
                in the queue is to be overwritten then it does not matter if the
                queue is full. */
                if self.uxMessagesWaiting < self.uxLength || xCopyPosition == queueOVERWRITE {
                    traceQUEUE_SEND!(&self);
                    self.copy_data_to_queue(pvItemToQueue, xCopyPosition);
                    trace!("Queue can be sent");

                    /* The queue is a member of a queue set, and posting
                    to the queue set caused a higher priority task to
                    unblock. A context switch is required. */
                    #[cfg(feature = "configUSE_QUEUE_SETS")]
                    match self.pxQueueSetContainer {
                        Some => {
                            if notify_queue_set_container(&self, &xCopyPosition) != false {
                                queueYIELD_IF_USING_PREEMPTION!();
                            } else {
                                mtCOVERAGE_TEST_MARKER!();
                            }
                        }
                        None => {
                            if list::list_is_empty(&self.xTasksWaitingToReceive) == false {
                                if task_queue::task_remove_from_event_list(
                                    &self.xTasksWaitingToReceive,
                                ) {
                                    queueYIELD_IF_USING_PREEMPTION!();
                                } else {
                                    mtCOVERAGE_TEST_MARKER!();
                                }
                            }
                        }
                    }

                    {
                        /* If there was a task waiting for data to arrive on the
                        queue then unblock it now. */
                        #![cfg(not(feature = "configUSE_QUEUE_SETS"))]
                        if !list::list_is_empty(&self.xTasksWaitingToReceive) {
                            if task_queue::task_remove_from_event_list(&self.xTasksWaitingToReceive)
                            {
                                /* The unblocked task has a priority higher than
                                our own so yield immediately.  Yes it is ok to do
                                this from within the critical section - the kernel
                                takes care of that. */
                                queueYIELD_IF_USING_PREEMPTION!();
                            } else {
                                mtCOVERAGE_TEST_MARKER!();
                            }
                        }
                        else {
                            mtCOVERAGE_TEST_MARKER!();
                        }
                    }
                    taskEXIT_CRITICAL!();
                    return Ok(()); //return pdPASS
                } else {
                    {
                        #![cfg(feature = "configUSE_MUTEXES")]
                        if self.ucQueueType == QueueType::Mutex || self.ucQueueType == QueueType::RecursiveMutex {
                            taskENTER_CRITICAL!();
                            {
                                let task_handle = self.transed_task_handle_for_mutex();
                                task_queue::task_priority_inherit(task_handle);
                            }
                            taskEXIT_CRITICAL!();
                        }
                        else {
                            mtCOVERAGE_TEST_MARKER!();
                        }
                    }
                    if xTicksToWait == 0 as TickType {
                        /* The queue was full and no block time is specified (or
                        the block time has expired) so leave now. */
                        taskEXIT_CRITICAL!();
                        /* Return to the original privilege level before exiting
                        the function. */
                        traceQUEUE_SEND_FAILED!(&self);
                        trace!("Queue Send: QueueFull");
                        return Err(QueueError::QueueFull);
                    } else if !xEntryTimeSet {
                        /* The queue was full and a block time was specified so
                        configure the timeout structure. */
                        task_queue::task_set_time_out_state(&mut xTimeOut);
                        xEntryTimeSet = true;
                    } else {
                        /* Entry time was already set. */
                        mtCOVERAGE_TEST_MARKER!();
                    }
                }
            }
            taskEXIT_CRITICAL!();

            /* Interrupts and other tasks can send to and receive from the queue
            now the critical section has been exited. */
            kernel::task_suspend_all();
            self.lock_queue();

            /* Update the timeout state to see if it has expired yet. */
            if !task_queue::task_check_for_timeout(&mut xTimeOut, &mut xTicksToWait) {
                if self.is_queue_full() {
                    traceBLOCKING_ON_QUEUE_SEND!(&self);
                    trace!("queue_generic_send place on event list");
                    task_queue::task_place_on_event_list(&self.xTasksWaitingToSend, xTicksToWait);

                    /* Unlocking the queue means queue events can effect the
                    event list.  It is possible	that interrupts occurring now
                    remove this task from the event	list again - but as the
                    scheduler is suspended the task will go onto the pending
                    ready last instead of the actual ready list. */
                    self.unlock_queue();

                    /* Resuming the scheduler will move tasks from the pending
                    ready list into the ready list - so it is feasible that this
                    task is already in a ready list before it yields - in which
                    case the yield will not cause a context switch unless there
                    is also a higher priority task in the pending ready list. */
                    if !kernel::task_resume_all() {
                        portYIELD_WITHIN_API!();
                    }
                } else {
                    /* Try again. */
                    self.unlock_queue();
                    kernel::task_resume_all();
                }
            } else {
                /* The timeout has expired. */
                self.unlock_queue();
                kernel::task_resume_all();

                traceQUEUE_SEND_FAILED!(self);
                return Err(QueueError::QueueFull);
            }
        }
    }

    /// # Description
    /// Post an item to a queue. It is safe to use this function from within an interrupt service routine.
    /// 
    /// * Implemented by:Ning Yuting
    /// * C implementation:queue.c 921-1069
    /// 
    /// # Argument
    /// `pvItemToQueue` - the item that is to be placed on the queue.
    /// `xCopyPosition` - the position that the item is to be placed.
    ///
    /// # Return
    /// * `Result` -Ok() if the data was successfully sent to the queue, otherwise errQUEUE_FULL.
    /// * `bool` - pxHigherPriorityTaskWoken is changed to be a return value. it is true if sending to the
    /// queue caused a task to unblock,otherwise it is false.`
    pub fn queue_generic_send_from_isr(
        &mut self,
        pvItemToQueue: T,
        xCopyPosition: BaseType,
    ) -> (Result<(), QueueError>, bool) {
        //原先参数const pxHigherPriorityTaskWoken: BaseType作为返回值的第二个元素，bool型
        //返回值改为struct

        let mut xReturn: Result<(), QueueError> = Ok(());
        let mut pxHigherPriorityTaskWoken: bool = false; //默认为false,下面一些情况改为true

        portASSERT_IF_INTERRUPT_PRIORITY_INVALID!();
        let uxSavedInterruptStatus: UBaseType = portSET_INTERRUPT_MASK_FROM_ISR!() as UBaseType;
        {
            if self.uxMessagesWaiting < self.uxLength || xCopyPosition == queueOVERWRITE {
                let cTxLock: i8 = self.cTxLock;
                traceQUEUE_SEND_FROM_ISR!(&self);
                self.copy_data_to_queue(pvItemToQueue, xCopyPosition);

                if cTxLock == queueUNLOCKED {
                    #[cfg(feature = "configUSE_QUEUE_SETS")]
                    match self.pxQueueSetContainer {
                        Some => {
                            if notify_queue_set_container(self, xCopyPosition) != false {
                                pxHigherPriorityTaskWoken = true
                            } else {
                                mtCOVERAGE_TEST_MARKER!();
                            }
                        }
                        None => {
                            if list::list_is_empty(&self.xTasksWaitingToReceive) == false {
                                if task_queue::task_remove_from_event_list(
                                    &self.xTasksWaitingToReceive,
                                ) != false
                                {
                                    pxHigherPriorityTaskWoken = true;
                                } else {
                                    mtCOVERAGE_TEST_MARKER!();
                                }
                            } else {
                                mtCOVERAGE_TEST_MARKER!();
                            }
                        }
                    }

                    {
                        #![cfg(not(feature = "configUSE_QUEUE_SETS"))]
                        if list::list_is_empty(&self.xTasksWaitingToReceive) == false {
                            if task_queue::task_remove_from_event_list(&self.xTasksWaitingToReceive)
                                != false
                            {
                                pxHigherPriorityTaskWoken = true;
                            } else {
                                mtCOVERAGE_TEST_MARKER!();
                            }
                        } else {
                            mtCOVERAGE_TEST_MARKER!();
                        }
                    }
                } else {
                    self.cTxLock = (cTxLock + 1) as i8;
                }
                xReturn = Ok(());
            } else {
                traceQUEUE_SEND_FROM_ISR_FAILED!(&self);
                xReturn = Err(QueueError::QueueFull);
            }
        }
        portCLEAR_INTERRUPT_MASK_FROM_ISR!(uxSavedInterruptStatus);
        (xReturn, pxHigherPriorityTaskWoken)
    }

    /// # Description
    /// Lock the queue.
    /// 
    /// * Implemented by:Ning Yuting
    /// * C implementation:queue.c 264-276
    /// 
    /// # Argument
    /// Nothing
    ///
    /// # Return
    /// Nothing
    pub fn lock_queue(&mut self) {
        //源码中为宏，改为Queue的方法
        taskENTER_CRITICAL!();
        {
            if self.cRxLock == queueUNLOCKED {
                self.cRxLock = queueLOCKED_UNMODIFIED;
            }
            if self.cTxLock == queueUNLOCKED {
                self.cTxLock = queueLOCKED_UNMODIFIED;
            }
        }
        taskEXIT_CRITICAL!();
    }

    /// # Description
    /// Unlock the queue
    /// 
    /// * Implemented by:Ning Yuting
    /// * C implementation:queue.c 1794-1911
    /// 
    /// # Argument
    /// Nothing
    /// 
    /// # Return
    /// Nothing
    fn unlock_queue(&mut self) {
        taskENTER_CRITICAL!();
        {
            let mut cTxLock: i8 = self.cTxLock;
            while cTxLock > queueLOCKED_UNMODIFIED {
                #[cfg(feature = "configUSE_QUEUE_SETS")]
                match self.pxQueueSetContainer {
                    Some => {
                        if notify_queue_set_container(self, queueSEND_TO_BACK) != false {
                            task_queue::task_missed_yield();
                        } else {
                            mtCOVERAGE_TEST_MARKER!();
                        }
                    }
                    None => {
                        if list::list_is_empty(&self.xTasksWaitingToReceive) == false {
                            if task_queue::task_remove_from_event_list(&self.xTasksWaitingToReceive)
                                != false
                            {
                                task_queue::task_missed_yield();
                            } else {
                                mtCOVERAGE_TEST_MARKER!();
                            }
                        } else {
                            break;
                        }
                    }
                }
                {
                    #![cfg(not(feature = "configUSE_QUEUE_SETS"))]
                    if list::list_is_empty(&self.xTasksWaitingToReceive) == false {
                        if task_queue::task_remove_from_event_list(&self.xTasksWaitingToReceive)
                            != false
                        {
                            task_queue::task_missed_yield();
                        } else {
                            mtCOVERAGE_TEST_MARKER!();
                        }
                    } else {
                        break;
                    }
                }

                cTxLock = cTxLock - 1;
            }
            self.cTxLock = queueUNLOCKED;
        }
        taskEXIT_CRITICAL!();

        taskENTER_CRITICAL!();
        {
            let mut cRxLock: i8 = self.cRxLock;
            while cRxLock > queueLOCKED_UNMODIFIED {
                if list::list_is_empty(&self.xTasksWaitingToReceive) == false {
                    if task_queue::task_remove_from_event_list(&self.xTasksWaitingToReceive)
                        != false
                    {
                        task_queue::task_missed_yield();
                    } else {
                        mtCOVERAGE_TEST_MARKER!();
                    }

                    cRxLock = cRxLock - 1;
                } else {
                    break;
                }
            }
            self.cRxLock = queueUNLOCKED;
        }
        taskEXIT_CRITICAL!();
    }

    /// # Description
    /// Receive an item from a queue.
    /// The item is received by copy and is returned by Ok(T);
    /// 
    /// * Implemented by:Ning Yuting
    /// * C implementation: queue.c 1237
    /// 
    /// # Argument
    /// * `xTicksToWait` - The maximum amount of time the task should block
    /// waiting for an item to receive should the queue be empty at the time
    /// of the call.It will return immediately if xTicksToWait is zero and the queue is empty.
    /// * `xJustPeeking` - whether the item will remain in the queue.
    ///
    /// # Return
    /// Ok(T) if an item was successfully received from the queue, otherwise QueueError::QueueEmpty.
    pub fn queue_generic_receive(
        &mut self,
        mut xTicksToWait: TickType,
        xJustPeeking: bool,
    ) -> Result<T, QueueError> {
        let mut xEntryTimeSet: bool = false;
        let mut xTimeOut: time_out = Default::default();
        /*when receive = give, it has to call the function task_priority_disinherit. It may require
         * yield.*/
        let mut xYieldRequired: bool = false;
        let mut buffer: Option<T>;
        #[cfg(all(feature = "xTaskGetSchedulerState", feature = "configUSE_TIMERS"))]
        assert!(
            !((kernel::task_get_scheduler_state() == SchedulerState::Suspended)
                && (xTicksToWait != 0))
        );
        /* This function relaxes the coding standard somewhat to allow return
	statements within the function itself.  This is done in the interest
	of execution time efficiency. */
        loop {
            trace!(
                "Enter function queue_generic_receive, TicksToWait:{}, Peeking: {}!",
                xTicksToWait,
                xJustPeeking
            );
            taskENTER_CRITICAL!();
            {
                let uxMessagesWaiting: UBaseType = self.uxMessagesWaiting;
                trace!(
                    "queue_generic_receive: uxMessageWaiting: {}",
                    uxMessagesWaiting
                );
                /* Is there data in the queue now?  To be running the calling task
                must be the highest priority task wanting to access the queue. */
                if uxMessagesWaiting > 0 as UBaseType {
                    /* Remember the read position in case the queue is only being
                       peeked. */
                    let pcOriginalReadPosition: UBaseType = self.QueueUnion; //QueueUnion represents pcReadFrom
                    buffer = self.copy_data_from_queue(); //
                    if xJustPeeking == false {
                        traceQUEUE_RECEIVE!(&self);
                        /* actually removing data, not just peeking. */
                        self.uxMessagesWaiting = uxMessagesWaiting - 1;

                        {
                            #![cfg(feature = "configUSE_MUTEXES")]
                            /*if uxQueueType == queueQUEUE_IS_MUTEX*/
                            if self.ucQueueType == QueueType::Mutex
                                || self.ucQueueType == QueueType::RecursiveMutex
                            {
                                let task_handle = self.transed_task_handle_for_mutex();
                                xYieldRequired = task_queue::task_priority_disinherit(task_handle);
                                self.pcQueue.pop_front();
                            } else {
                                mtCOVERAGE_TEST_MARKER!();
                            }
                        }
                        trace!("queue_generic_receive -- line 498");
                        if list::list_is_empty(&self.xTasksWaitingToSend) == false {
                            if task_queue::task_remove_from_event_list(&self.xTasksWaitingToSend)
                                != false
                            {
                                queueYIELD_IF_USING_PREEMPTION!();
                            } else {
                                trace!("queue_generic_receive -- line 504");
                                mtCOVERAGE_TEST_MARKER!();
                            }
                        } else if xYieldRequired == true {
                            /* This path is a special case that will only get
                             * executed if the task was holding multiple mutexes
                             * and the mutexes were given back in an order that is
                             * different to that in which they were taken. */
                            queueYIELD_IF_USING_PREEMPTION!();
                        } else {
                            trace!("queue_generic_receive -- line 508");
                            mtCOVERAGE_TEST_MARKER!();
                        }
                    } else {
                        traceQUEUE_PEEK!(&self);
                        /* The data is not being removed, so reset the read
                        pointer. */
                        self.QueueUnion = pcOriginalReadPosition; //QueueUnnion represents pcReadFrom
                        /* The data is being left in the queue, so see if there are
                           any other tasks waiting for the data. */
                        if list::list_is_empty(&self.xTasksWaitingToReceive) != false {
                            if task_queue::task_remove_from_event_list(&self.xTasksWaitingToReceive)
                                != false
                            {
                                queueYIELD_IF_USING_PREEMPTION!();
                            } else {
                                mtCOVERAGE_TEST_MARKER!();
                            }
                        } else {
                            mtCOVERAGE_TEST_MARKER!();
                        }
                    }
                    taskEXIT_CRITICAL!();
                    trace!("queue_generic_receive -- line 529");
                    return Ok(buffer.unwrap_or_else(|| panic!("buffer is empty!")));
                } else {
                    if xTicksToWait == 0 as TickType {
                        /* The queue was empty and no block time is specified (or
                        the block time has expired) so leave now. */
                        taskEXIT_CRITICAL!();
                        traceQUEUE_RECEIVE_FAILED!(&self);
                        return Err(QueueError::QueueEmpty);
                    } else if xEntryTimeSet == false {
                        /* The queue was empty and a block time was specified so
                        configure the timeout structure. */
                        task_queue::task_set_time_out_state(&mut xTimeOut);
                        xEntryTimeSet = true;
                    } else {
                        /* Entry time was already set. */
                        mtCOVERAGE_TEST_MARKER!();
                    }
                }
            }
            taskEXIT_CRITICAL!();
            trace!("queue_generic_receive -- line 553");
            kernel::task_suspend_all();
            self.lock_queue();
            trace!("queue_generic_receive -- line 556");
            /* Update the timeout state to see if it has expired yet. */
            if task_queue::task_check_for_timeout(&mut xTimeOut, &mut xTicksToWait) == false {
                if self.is_queue_empty() != false {
                    traceBLOCKING_ON_QUEUE_RECEIVE!(&self);
                    task_queue::task_place_on_event_list(
                        &self.xTasksWaitingToReceive,
                        xTicksToWait,
                    );
                    self.unlock_queue();
                    if kernel::task_resume_all() == false {
                        portYIELD_WITHIN_API!();
                    } else {
                        mtCOVERAGE_TEST_MARKER!();
                    }
                } else {
                    self.unlock_queue();
                    kernel::task_resume_all();
                }
                trace!("queue_generic_receive -- line 589");
            } else {
                self.unlock_queue();
                kernel::task_resume_all();
                if self.is_queue_empty() != false {
                    traceQUEUE_RECEIVE_FAILED!(&self);
                    return Err(QueueError::QueueEmpty);
                } else {
                    mtCOVERAGE_TEST_MARKER!();
                }
            }
        }
    }

    pub fn copy_data_from_queue(&mut self) -> Option<T> {
        self.QueueUnion += 1; //QueueUnion represents pcReadFrom in the original code
        if self.QueueUnion >= self.pcTail {
            self.QueueUnion = self.pcHead;
        } else {
            mtCOVERAGE_TEST_MARKER!();
        }
        let ret_val = self.pcQueue.get(self.QueueUnion as usize).cloned();
        Some(ret_val.unwrap())
    }

    pub fn copy_data_to_queue(&mut self, pvItemToQueue: T, xPosition: BaseType) /*-> bool*/
    {
        /* This function is called from a critical section. */
        let mut uxMessagesWaiting: UBaseType = self.uxMessagesWaiting;

        {
            #![cfg(feature = "configUSE_MUTEXES")]
            if self.ucQueueType == QueueType::Mutex || self.ucQueueType == QueueType::RecursiveMutex
            {
                let mutex_holder = transed_task_handle_to_T(task_increment_mutex_held_count());
                self.pcQueue.insert(0, mutex_holder);
            } else {
                mtCOVERAGE_TEST_MARKER!();
            }
        }

        if xPosition == queueSEND_TO_BACK {
            if self.ucQueueType != QueueType::Mutex && self.ucQueueType != QueueType::RecursiveMutex {
                self.pcQueue.insert(self.pcWriteTo as usize, pvItemToQueue);
            }
            else {
            }
            self.pcWriteTo = self.pcWriteTo + 1;

            if self.pcWriteTo >= self.pcTail {
                self.pcWriteTo = self.pcHead;
            } else {
                mtCOVERAGE_TEST_MARKER!();
            }
        } else {
            if self.ucQueueType != QueueType::Mutex && self.ucQueueType != QueueType::RecursiveMutex {
                self.pcQueue.insert(self.QueueUnion as usize, pvItemToQueue); //QueueUnion represents pcReadFrom
            }
            else {
            }
            self.QueueUnion = self.QueueUnion - 1;
            if self.QueueUnion < self.pcHead {
                self.QueueUnion = self.pcTail - 1;
            } else {
                mtCOVERAGE_TEST_MARKER!();
            }

            if xPosition == queueOVERWRITE {
                if uxMessagesWaiting > 0 as UBaseType {
                    /* An item is not being added but overwritten, so subtract
                    one from the recorded number of items in the queue so when
                    one is added again below the number of recorded items remains
                    correct. */
                    uxMessagesWaiting = uxMessagesWaiting - 1;
                } else {
                    mtCOVERAGE_TEST_MARKER!();
                }
            } else {
                mtCOVERAGE_TEST_MARKER!();
            }
        }
        self.uxMessagesWaiting = uxMessagesWaiting + 1;
    }

    /// # Description
    /// To know whether the queue is empty.
    ///
    /// * Implemented by:Ning Yuting
    /// * C implementation: queue.c 1914
    ///
    /// # Argument:
    /// Nothing
    ///
    /// # Return:
    /// `bool` - true if the queue was empty.
    pub fn is_queue_empty(&self) -> bool {
        let mut xReturn: bool = false;
        taskENTER_CRITICAL!();
        {
            if self.uxMessagesWaiting == 0 as UBaseType {
                xReturn = true;
            }
        }
        taskEXIT_CRITICAL!();
        xReturn
    }

    /// # Description
    /// To know whether the queue is full.
    ///
    /// * Implemented by:Lei Siqi
    /// 
    /// # Argument
    /// Nothing
    ///
    /// # Return
    /// `bool` - true if the queue if full.
    pub fn is_queue_full(&self) -> bool {
        let mut xReturn: bool = false;
        taskENTER_CRITICAL!();
        {
            if self.uxMessagesWaiting == self.uxLength {
                xReturn = true;
            }
        }
        taskEXIT_CRITICAL!();
        xReturn
    }

    pub fn initialise_count(&mut self, initial_count: UBaseType) {
        self.uxMessagesWaiting = initial_count;
    }

    pub fn QueueUnion_decrease(&mut self) {
        self.QueueUnion = self.QueueUnion - 1;
    }

    pub fn QueueUnion_increase(&mut self) {
        self.QueueUnion = self.QueueUnion + 1;
    }

    pub fn is_QueueUnion_zero(&self) -> bool {
        if self.QueueUnion == 0 as UBaseType {
            return true;
        } else {
            return false;
        }
    }

    pub fn get_recursive_count(&self) -> UBaseType {
        self.QueueUnion
    }

    /* `new` has two arguments now:length, QueueType.
     * Remember to add QueueType when using it.
     */
    /// # Description
    /// Create a new queue. Same to queue_generic_create.
    /// * Implemented by:Ning Yuting
    pub fn new(uxQueueLength: UBaseType, QueueType: QueueType) -> Self {
        QueueDefinition::queue_generic_create(uxQueueLength, QueueType)
    }

    #[cfg(feature = "configUSE_TRACE_FACILITY")]
    pub fn get_queue_number(&self) -> UBaseType {
        self.uxQueueNumber
    }

    #[cfg(feature = "configUSE_QUEUE_SETS")]
    fn notify_queue_set_container(&self, xCopyPosition: BaseType) {
        unimplemented!();
    }

    /// # Description
    /// Transform pcQueue.0 to Option<task_control::TaskHandle>
    ///
    /// * Implemented by:Ning Yuting
    ///
    /// # Arguments:
    /// Nothing
    ///
    /// # Return:
    /// `Option<task_control::TaskHandle>` - the transformed TaskHandle
    pub fn transed_task_handle_for_mutex(&self) -> Option<task_control::TaskHandle> {
        /* use unsafe to get transed_task_handle for mutex
         * inplemented by: Ning Yuting
         */
        if self.pcQueue.get(0).cloned().is_some() {
            let untransed_task_handle = self.pcQueue.get(0).cloned().unwrap();
            trace!("successfully get the task handle");
            let untransed_task_handle = Box::new(untransed_task_handle);
            let mut task_handle: Option<task_control::TaskHandle>;
            unsafe {
                let transed_task_handle = std::mem::transmute::<
                    Box<T>,
                    Box<Option<task_control::TaskHandle>>,
                >(untransed_task_handle);
                task_handle = *transed_task_handle
            }
            task_handle
        }
        else {
            None
        }
    }
}

/// # Description
/// Transform Option<task_control::TaskHandle> to T
///
/// * Implemented by:Ning Yuting
///
/// # Arguments:
/// `task_handle` - the TaskHandle that is to be transformed.
///
/// # Return:
/// `T` - the transformed T.
fn transed_task_handle_to_T<T>(task_handle: Option<task_control::TaskHandle>) -> T {
    /* use unsafe to transmute Option<TaskHandle> to T type*/
    let mut T_type: T;
    let task_handle = Box::new(task_handle);
    unsafe {
        let transed_T =
            std::mem::transmute::<Box<Option<task_control::TaskHandle>>, Box<T>>(task_handle);
        T_type = *transed_T;
    }
    T_type
}

#[macro_export]
macro_rules! queueYIELD_IF_USING_PREEMPTION {
    () => {
        #[cfg(feature = "configUSE_PREEMPTION")]
        portYIELD_WITHIN_API!();
    };
}
use crate::port::*;
use crate::queue::*;
use crate::queue_h::*;
use crate::task_control::*;
use crate::*;
use std::cell::UnsafeCell;

pub struct Semaphore(UnsafeCell<QueueDefinition<Option<TaskHandle>>>);
unsafe impl Send for Semaphore {}
unsafe impl Sync for Semaphore {}

impl Semaphore {
    /// # Descrpition
    /// Create a new mutex type semaphore instance.
    ///
    /// * Implemented by:Ning Yuting
    /// * C implementation:queue.c 504-515
    ///
    /// # Arguments:
    /// Nothing
    ///
    /// # Return:
    /// The created mutex.
    pub fn new_mutex() -> Self {
        Semaphore(UnsafeCell::new(QueueDefinition::new(1, QueueType::Mutex)))
    }

    /// # Description
    /// Get the mutex holder.
    ///
    /// * Implemented by:Lei Siqi
    ///
    /// # Arguments:
    /// Nothing
    ///
    /// # Return:
    /// `Option<task_control::TaskHandle>` - the holder of the mutex
    #[cfg(all(
        feature = "configUSE_MUTEXES",
        feature = "INCLUDE_xSemaphoreGetMutexHolder"
    ))]
    pub fn get_mutex_holder(&self) -> Option<task_control::TaskHandle> {
        let mut mutex_holder: Option<task_control::TaskHandle>;
        taskENTER_CRITICAL!();
        {
            unsafe {
                let inner = self.0.get();
                mutex_holder = (*inner).queue_generic_receive(0, true).unwrap();
            }
        }
        taskEXIT_CRITICAL!();
        mutex_holder
    }

    /// # Description
    /// Release a semaphore.
    ///
    /// * Implemented by:Ning Yuting & Lei Siqi
    /// * C implementation:semphr.h 489 
    ///
    /// # Arguments:
    /// Nothing
    /// 
    /// # Return:
    /// Ok(T) if the semaphore was released, otherwise QueueError::QueueEmpty.
    pub fn semaphore_up(&self) -> Result<Option<TaskHandle>, QueueError> {
        unsafe {
            trace!("Semaphore up runs!");
            let inner = self.0.get();
            trace!("Semaphore up get finished!");
            (*inner).queue_generic_receive(semGIVE_BLOCK_TIME, false)
        }
    }

    /// # Description
    /// Obtain a semaphore.
    ///
    /// * Implemented by:Ning Yuting & Lei Siqi
    /// * C implementation:semphr.h 331
    ///
    /// # Arguments:
    /// `xBlockTime` - The time in ticks to wait for the semaphore to become available.
    /// A block time of zero can be used to poll the semaphore.
    /// A block time of portMAX_DELAY can be used to block indefinitely.
    ///
    /// # Return:
    /// Ok() if the semaphore was obtained, otherwise errQUEUE_FULL.
    pub fn semaphore_down(&self, xBlockTime: TickType) -> Result<(), QueueError> {
        unsafe {
            let inner = self.0.get();
            (*inner).queue_generic_send(None, xBlockTime, queueSEND_TO_BACK)
        }
    }

    /// # Description
    /// Create a binary semaphore.
    ///
    /// * Implemented by:Ning Yuting
    /// * C implementation:semphr.h 135-144
    ///
    /// # Arguments:
    /// Nothing
    ///
    /// # Return:
    /// The created binary semaphore.
    pub fn create_binary() -> Self {
        Semaphore(UnsafeCell::new(QueueDefinition::new(
            1,
            QueueType::BinarySemaphore,
        )))
    }

    /// # Description
    /// Create a counting semaphore.
    ///
    /// * Implemented by:Ning Yuting
    /// * C implementation:semphr.h 1039-1041
    ///
    /// # Arguments:
    /// `max_count` - The maximum count value that can be reached. When the semaphore reaches 
    /// this value it can no longer be 'given'.
    ///
    /// # Return
    /// The created counting semaphore.
    pub fn create_counting(max_count: UBaseType /*,initial_count:UBaseType*/) -> Self {
        let mut counting_semphr = Semaphore(UnsafeCell::new(QueueDefinition::new(
            max_count,
            QueueType::CountingSemaphore,
        )));
        unsafe {
            let inner = counting_semphr.0.get();
            (*inner).initialise_count(0);
        }
        //traceCREATE_COUNTING_SEMAPHORE!();
        counting_semphr
    }

    /// # Description
    /// Created a recursive mutex.
    ///
    /// * Implemented by:Ning Yuting
    /// * C implementation:semphr.h 886-888
    ///
    /// # Argument
    /// Nothing
    ///
    /// # Return
    /// The created recursive mutex.
    pub fn create_recursive_mutex() -> Self {
        Semaphore(UnsafeCell::new(QueueDefinition::new(
            1,
            QueueType::RecursiveMutex,
        )))
    }

    /// # Description
    /// Release a recursive mutex.
    ///
    /// * Implemented by:Ning Yuting
    /// * C implementation:queue.c 570-622
    ///
    /// # Arguments:
    /// Nothing
    /// 
    /// # Return
    /// `bool` - true if the recursive mutex was released.
    pub fn up_recursive(&self) -> bool {
        unsafe {
            let inner = self.0.get();
            if (*inner).transed_task_handle_for_mutex().unwrap().clone()
                == get_current_task_handle!()
            {
                traceGIVE_MUTEX_RECURSIVE!(*inner);
                (*inner).QueueUnion_decrease();
                if (*inner).is_QueueUnion_zero() {
                    (*inner).queue_generic_receive(semGIVE_BLOCK_TIME, false);
                } else {
                    mtCOVERAGE_TEST_MARKER!();
                }
                return true;
            } else {
                traceGIVE_MUTEX_RECURSIVE_FAILED!(*inner);
                return false;
            }
        }
    }

    /// # Description
    /// Obtain a recursive mutex.
    ///
    /// * Implemented by:Ning Yuting & Lei Siqi
    /// * C implementation:queue.c 625-664
    ///
    /// # Arguments:
    /// `ticks_to_wait` - The time in ticks to wait for the semaphore to become available.
    /// A block time of zero can be used to poll the semaphore.
    ///
    /// # Return:
    /// `bool` - true if the recursive mutex was obtained.
    pub fn down_recursive(&self, ticks_to_wait: TickType) -> bool {
        let mut xReturn: bool = false;
        unsafe {
            let inner = self.0.get();
            traceTAKE_MUTEX_RECURSIVE!(*inner);
            trace!("Ready to get recursive mutex holder");
            let mutex_holder = (*inner).transed_task_handle_for_mutex();
            trace!("Get recursive mutex holder successfully");
            if mutex_holder.is_some()
            {
                if mutex_holder.unwrap().clone() == get_current_task_handle!() {
                    trace!("Not First Time get this mutex");
                    (*inner).QueueUnion_increase();
                    xReturn = false;
                }
            } 
            // else {
                trace!("First Time get this mutex");
                match (*inner).queue_generic_send(None, ticks_to_wait, queueSEND_TO_BACK) {
                    Ok(x) => {
                        (*inner).QueueUnion_increase();
                        xReturn = true;
                    }
                    Err(x) => {
                        traceTAKE_MUTEX_RECURSIVE_FAILED!(*inner);
                        xReturn = false;
                    }
                }
            // }
        }
        return xReturn;
    }

    /// # Description
    /// Get the recursive count of a recursive mutex.
    ///
    /// * Implemented by:Lei Siqi
    ///
    /// # Arguments:
    /// Nothing
    ///
    /// # Return:
    /// `UBaseType` - the recursive count of the recursive mutex.
    pub fn get_recursive_count(&self) -> UBaseType {
        unsafe {
            let inner = self.0.get();
            (*inner).get_recursive_count()
        }
    }
}
use crate::kernel;
use crate::list;
use crate::port;
use crate::port::{BaseType, TickType, UBaseType};
use crate::task_control;
use crate::task_control::TaskHandle;
use crate::task_global::*;
use crate::task_queue;
use crate::task_queue::taskEVENT_LIST_ITEM_VALUE_IN_USE;
use crate::trace::*;
use crate::*;

macro_rules! get_tcb_from_handle_inAPI {
    ($task:expr) => {
        match $task {
            Some(t) => t,
            None => get_current_task_handle!(),
        }
    };
}

///  INCLUDE_uxTaskPriorityGet must be defined as 1 for this function to be available.
///  See the configuration section for more information.
///
///  Obtain the priority of any task.
///
///
/// * Implemented by: Fan Jinhao
///
/// # Arguments:
///  `xTask` Handle of the task to be queried.  Passing a NULL
///  handle results in the priority of the calling task being returned.
///
///
/// * Return:
///  The priority of xTask.
///
pub fn task_priority_get(xTask: Option<TaskHandle>) -> UBaseType {
    let mut uxReturn: UBaseType = 0;
    taskENTER_CRITICAL!();
    {
        let pxTCB = get_tcb_from_handle_inAPI!(xTask);
        uxReturn = pxTCB.get_priority();
    }
    taskEXIT_CRITICAL!();
    return uxReturn;
}

///  INCLUDE_vTaskPrioritySet must be defined as 1 for this function to be available.
///  See the configuration section for more information.
///
///  Set the priority of any task.
///
///  A context switch will occur before the function returns if the priority
///  being set is higher than the currently executing task.
///
///
/// * Implemented by: Fan Jinhao
///
/// # Arguments:
///  `xTask` Handle to the task for which the priority is being set.
///  Passing a NULL handle results in the priority of the calling task being set.
///
///  `uxNewPriority` The priority to which the task will be set.
///
///
/// * Return:
///
pub fn task_priority_set(xTask: Option<TaskHandle>, uxNewPriority: UBaseType) {
    let mut uxNewPriority = uxNewPriority;
    let mut xYieldRequired: bool = false;
    let mut uxCurrentBasePriority: UBaseType = 0;
    let mut uxPriorityUsedOnEntry: UBaseType = 0;

    //valid ensure
    if uxNewPriority >= configMAX_PRIORITIES!() as UBaseType {
        uxNewPriority = configMAX_PRIORITIES!() as UBaseType - 1 as UBaseType;
    } else {
        mtCOVERAGE_TEST_MARKER!();
    }
    taskENTER_CRITICAL!();

    {
        let mut pxTCB = get_tcb_from_handle_inAPI!(xTask);
        traceTASK_PRIORITY_SET!(&pxTCB, &uxNewPriority); // crate?

        {
            #![cfg(feature = "configUSE_MUTEXES")]
            uxCurrentBasePriority = pxTCB.get_base_priority();
        }

        {
            #![cfg(not(feature = "configUSE_MUTEXES"))]
            uxCurrentBasePriority = pxTCB.get_priority();
        }

        if uxCurrentBasePriority != uxNewPriority {
            // change the Priority ;
            if pxTCB != get_current_task_handle!() {
                if uxNewPriority >= get_current_task_priority!() {
                    xYieldRequired = true;
                } else {
                    mtCOVERAGE_TEST_MARKER!();
                }
            } else {;            } // 当前正在执行的task已是最高优先级
        } else if pxTCB == get_current_task_handle!() {
            xYieldRequired = true;
        } else {;        }
        // 其他task优先级设置不需要yield    ???

        {
            #![cfg(feature = "configUSE_MUTEXES")]
            if pxTCB.get_base_priority() == pxTCB.get_priority() {
                pxTCB.set_priority(uxNewPriority);
            } else {
                mtCOVERAGE_TEST_MARKER!();
            }
            pxTCB.set_base_priority(uxNewPriority);
        }
        #[cfg(not(feature = "configUSE_MUTEXS"))]
        pxTCB.set_priority(uxNewPriority);

        let event_list_item = pxTCB.get_event_list_item();
        let state_list_item = pxTCB.get_state_list_item();

        if (list::get_list_item_value(&event_list_item) & taskEVENT_LIST_ITEM_VALUE_IN_USE) == 0 {
            list::set_list_item_value(
                &event_list_item,
                (configMAX_PRIORITIES!() as TickType - uxNewPriority as TickType),
            );
        } else {
            mtCOVERAGE_TEST_MARKER!();
        }

        if list::is_contained_within(
            &READY_TASK_LISTS[uxPriorityUsedOnEntry as usize],
            &state_list_item,
        ) {
            if list::list_remove(state_list_item) == 0 as UBaseType {
                portRESET_READY_PRIORITY!(uxPriorityUsedOnEntry, uxTopReadyPriority);
            } else {
                mtCOVERAGE_TEST_MARKER!();
            }
            pxTCB.add_task_to_ready_list();
        } else {
            mtCOVERAGE_TEST_MARKER!();
        }

        if xYieldRequired != false {
            taskYIELD_IF_USING_PREEMPTION!();
        } else {
            mtCOVERAGE_TEST_MARKER!();
        }
    }

    taskEXIT_CRITICAL!();
}

/*
   pub fn task_get_system_state(pxTaskStatusArray:&TaskStatus , uxArraySize:UBaseType , pulTotalRunTime:u32) -> UBaseType
   {
   let mut uxtask:UBaseType = 0 ;
   let mut uxqueue = configmax_priorities!();
   kernel::task_suspend_all();      // ???
   {
/* is there a space in the array for each task in the system? */
if uxarraysize >= uxcurrentnumberoftasks
{
    // while 实现do while
    uxqueue = uxqueue - 1;
    uxtask += prvlisttaskswithinsinglelist( &( pxtaskstatusarray[ uxtask ] ), &( pxreadytaskslists[ uxqueue ] ), eready );
    while uxqueue > ( ubasetype ) tskidle_priority  /*lint !e961 misra exception as the casts are only redundant for some ports. */
    {
        uxqueue--;
        uxtask += prvlisttaskswithinsinglelist( &( pxtaskstatusarray[ uxtask ] ), &( pxreadytaskslists[ uxqueue ] ), eready );
    }

    uxtask += prvlisttaskswithinsinglelist( &( pxtaskstatusarray[ uxtask ] ), pxdelayedtasklist as &list , eblocked );
    uxtask += prvlisttaskswithinsinglelist( &( pxtaskstatusarray[ uxtask ] ), pxoverflowdelayedtasklist as &list , eblocked );

    #[cfg( feature = "include_vtaskdelete" )]
    uxtask += prvlisttaskswithinsinglelist( &( pxtaskstatusarray[ uxtask ] ), &xtaskswaitingtermination, edeleted );

    #[cfg( feature = "include_vtasksuspend" )]
    uxtask += prvlisttaskswithinsinglelist( &( pxtaskstatusarray[ uxtask ] ), &xsuspendedtasklist, esuspended );

    {
        #![cfg( feature = "configgenerate_run_time_stats" )]
        if pultotalruntime != null
        {
            #[cfg( feature = "portalt_get_run_time_counter_value" )]
            portalt_get_run_time_counter_value!( ( &pultotalruntime ) );
            #[cfg(not( feature = "portalt_get_run_time_counter_value" ))]
            &pultotalruntime = portget_run_time_counter_value!();
        }
    }
    {
        #![cfg(not( feature = "configgenrate_run_time_stats" ))]
        if( pultotalruntime != null )
            &pultotalruntime = 0;                               // 用&解除引用
    }
}
else {
    mtcoverage_test_marker!();
}
}
kernel::xtaskresumeall();
return uxtask;
}

pub fn task_test_info(xTask:Option<&TaskHandle>, pxTaskStatus:&TaskStatus, xGetFreeStackSpace:BaseType, eState:TaskState)
{

    /* xTask is NULL then get the state of the calling task. */
    let pxTCB = get_tcb_from_handle!( xTask );

    pxTaskStatus.xHandle = pxTCB;
    pxTaskStatus.task_name = &( pxTCB.task_name [ 0 ] );
    pxTaskStatus.uxCurrentPriority = pxTCB.task_priority;
    pxTaskStatus.pxStackBase = pxTCB.stack_pose;
    pxTaskStatus.xTaskNumber = pxTCB.tcb_Number;

    #[cfg( feature = "configUSE_MUTEXES" )]
    pxTaskStatus.base_priority = pxTCB.base_priority;
    #[cfg(not( feature = "configUSE_MUTEXES" ))]
    pxTaskStatus.base_priority = 0;

    #[cfg ( feature = "configGENERATE_RUN_TIME_STATS" )]
    pxTaskStatus.ulRunTimeCounter = pxTCB.ulRunTimeCounter;
    #[cfg(not( feature = "configGENERATE_RUN_TIME_STATS" ))]
    pxTaskStatus.ulRunTimeCounter = 0;

    /* Obtaining the task state is a little fiddly, so is only done if the
       value of eState passed into this function is eInvalid - otherwise the
       state is just set to whatever is passed in. */
    if eState != eInvalid
    {
        if pxTCB == &CurrentTCB {
            pxTaskStatus.eCurrentState = eRunning;
        } else
        {
            pxTaskStatus.eCurrentState = eState;

            {
                #![cfg(feature = "INCLUDE_vTaskSuspend" )]
                /* If the task is in the suspended list then there is a
                   chance it is actually just blocked indefinitely - so really
                   it should be reported as being in the Blocked state. */
                if eState == eSuspended
                {
                    vTaskSuspendAll();

                    {
                        if listLIST_ITEM_CONTAINER( &( pxTCB.xEventListItem ) ) != NULL
                        {
                            pxTaskStatus.eCurrentState = eBlocked;
                        }
                    }

                    ( void ) xTaskResumeAll();
                }
            }
        }
    }
    else
    {
        pxTaskStatus.eCurrentState = eTaskGetState( pxTCB );
    }

    /* Obtaining the stack space takes some time, so the xGetFreeStackSpace
       parameter is provided to allow it to be skipped. */
    if xGetFreeStackSpace != pdFALSE
    {
        if portSTACK_GROWTH > 0 {
            pxTaskStatus.usStackHighWaterMark = prvTaskCheckFreeStackSpace( pxTCB.pxEndOfStack as &i8 );
        } else {
            pxTaskStatus.usStackHighWaterMark = prvTaskCheckFreeStackSpace( pxTCB.pxStack as &i8 );
        }
    }
    else
    {
        pxTaskStatus.usStackHighWaterMark = 0;
    }
}


pub fn task_get_application_task_tag(xTask:TaskHandle) -> UBaseType
{
    let mut xReturn:UBaseType = 0 ;      // TaskHookFunction
    let mut pxTCB = get_tcb_from_handle_inAPI!(&xTask) ;
    taskENTER_CRITICAL!() ;
    xReturn = pxTCB.get_task_tag ;
    taskEXIT_CRITICAL!() ;
    xReturn ;
}

pub fn task_get_handle(pcNameToQuery:&char) -> &TaskHandle
{
    let mut uxQueue:UBaseType = configMAX_PRIORITIES;
    let mut pxTCB:&TCB = 0 ;

    vTaskSuspendAll();
    {
        /* Search the ready lists. */
        while uxQueue > ( UBaseType_t ) tskIDLE_PRIORITY  /*lint !e961 MISRA exception as the casts are only redundant for some ports. */
        {
            uxQueue--;
            pxTCB = prvSearchForNameWithinSingleList( ( List_t * ) &( pxReadyTasksLists[ uxQueue ] ), pcNameToQuery );

            if pxTCB != NULL
            {
                /* Found the handle. */
                break;
            }
        }

        /* Search the delayed lists. */
        if pxTCB == NULL {
            pxTCB = prvSearchForNameWithinSingleList( ( List_t * ) pxDelayedTaskList, pcNameToQuery );
        } else {
            pxTCB = prvSearchForNameWithinSingleList( ( List_t * ) pxOverflowDelayedTaskList, pcNameToQuery );
        }

        {
            #![cfg ( INCLUDE_vTaskSuspend == 1 )]
            if pxTCB == NULL
                pxTCB = prvSearchForNameWithinSingleList( &xSuspendedTaskList, pcNameToQuery );
        }

        {
            #![cfg( INCLUDE_vTaskDelete == 1 )]
            if pxTCB == NULL
                /* Search the deleted list. */{
                    pxTCB = prvSearchForNameWithinSingleList( &xTasksWaitingTermination, pcNameToQuery );
                }
        }
    }
    xTaskResumeAll();

    return pxTCB;
}

pub fn task_get_idle_task_handle() -> &TaskHandle
{
    /* If xTaskGetIdleTaskHandle() is called before the scheduler has been
       started, then xIdleTaskHandle will be NULL. */
    return IdleTaskHandle;
}

pub fn task_get_stack_high_water_mark(xtask:Option<&TaskHandle>) -> UBaseType
{
    let mut pucEndOfStack = 0;
    let mut uxReturn:UBaseType = 0;

    let pxTCB:&TCB = get_tcb_from_handle(xtask);

    if portSTACK_GROWTH < 0 {
        pucEndOfStack = pxTCB.pxStack;
    } else {
        pucEndOfStack = pxTCB.pxEndOfStack;
    }

    uxReturn = ( UBaseType )prvTaskCheckFreeStackSpace( pucEndOfStack );

    return uxReturn;
}
*/
use crate::kernel::*;
use crate::list;
use crate::list::ItemLink; use crate::list::*;
use crate::port::*;
use crate::projdefs::FreeRtosError;
use crate::task_global::*;
use crate::*;
use std::boxed::FnBox;
use std::mem;
use std::sync::{Arc, RwLock, Weak};

/* Task states returned by eTaskGetState. */
#[derive(Copy, Clone, Debug)]
#[repr(u8)]
pub enum task_state {
    running = 0,
    ready = 1,
    blocked = 2,
    suspended = 3,
    deleted = 4,
}

pub enum updated_top_priority {
    Updated,
    Notupdated,
}

#[derive(Debug)]
pub struct task_control_block {
    //* basic information
    state_list_item: ItemLink,
    event_list_item: ItemLink,
    task_priority: UBaseType,
    task_stacksize: UBaseType,
    task_name: String,
    // `stack_pos` is StackType because raw pointer can't be sent between threads safely.
    stack_pos: StackType,

    //* end of stack
    // #[cfg(portStack_GROWTH)]{}
    // end_of_stack: *mut StackType,

    //* nesting
    #[cfg(feature = "portCRITICAL_NESTING_IN_TCB")]
    critical_nesting: UBaseType,

    //* reverse priority
    #[cfg(feature = "configUSE_MUTEXES")]
    base_priority: UBaseType,
    #[cfg(feature = "configUSE_MUTEXES")]
    mutexes_held: UBaseType,

    #[cfg(feature = "configGENERATE_RUN_TIME_STATS")]
    runtime_counter: TickType,

    //* notify information
    #[cfg(feature = "configUSE_TASK_NOTIFICATIONS")]
    notified_value: u32,
    #[cfg(feature = "configUSE_TASK_NOTIFICATIONS")]
    notify_state: u8,
    #[cfg(feature = "INCLUDE_xTaskAbortDelay")]
    delay_aborted: bool,
}

pub type TCB = task_control_block;
pub type Task = task_control_block;
impl task_control_block {
    pub fn new() -> Self {
        task_control_block {
            state_list_item: Default::default(),
            event_list_item: Default::default(),
            task_priority: 1,
            task_stacksize: configMINIMAL_STACK_SIZE!(),
            task_name: String::from("Unnamed"),
            stack_pos: 0,

            //* nesting
            #[cfg(feature = "portCRITICAL_NESTING_IN_TCB")]
            critical_nesting: 0,

            //* reverse priority
            #[cfg(feature = "configUSE_MUTEXES")]
            base_priority: 0,
            #[cfg(feature = "configUSE_MUTEXES")]
            mutexes_held: 0,

            #[cfg(feature = "configGENERATE_RUN_TIME_STATS")]
            runtime_counter: 0,

            //* notify information
            #[cfg(feature = "configUSE_TASK_NOTIFICATIONS")]
            notified_value: 0,
            #[cfg(feature = "configUSE_TASK_NOTIFICATIONS")]
            notify_state: 0,
            #[cfg(feature = "INCLUDE_xTaskAbortDelay")]
            delay_aborted: false,
        }
    }

    /// * Descrpition:
    /// Reset the name of a TCB.
    ///
    /// * Implemented by: Fan Jinhao
    ///
    /// # Arguments:
    ///  `name` A descriptive name for the task.  This is mainly used to
    ///  facilitate debugging.  Max length defined by configMAX_TASK_NAME_LEN - default
    ///  is 16.
    ///
    /// # Return:
    /// Return a TCB with new name.

    pub fn name(mut self, name: &str) -> Self {
        self.task_name = name.to_owned().to_string();
        self
    }

    /// * Descrpition:
    /// Reset the stacksize of a TCB.
    ///
    /// * Implemented by: Fan Jinhao
    ///
    /// # Arguments:
    ///  `stacksize` The size of the task stack specified as the number of
    ///  variables the stack can hold - not the number of bytes.  For example, if
    ///  the stack is 16 bits wide and usStackDepth is defined as 100, 200 bytes
    ///  will be allocated for stack storage.
    ///
    /// # Return:
    /// Return a TCB with new stacksize.

    pub fn stacksize(mut self, stacksize: UBaseType) -> Self {
        self.task_stacksize = stacksize;
        self
    }

    /// * Descrpition:
    /// Reset the name of a priority.
    ///
    /// * Implemented by: Fan Jinhao
    ///
    /// # Arguments:
    ///  `priority` The priority at which the task should run.  Systems that
    ///  include MPU support can optionally create tasks in a privileged (system)
    ///  mode by setting bit portPRIVILEGE_BIT of the priority parameter.  For
    ///  example, to create a privileged task at priority 2 the uxPriority parameter
    ///  should be set to ( 2 | portPRIVILEGE_BIT ).
    ///
    /// # Return:
    /// Return a TCB with new priority.

    pub fn priority(mut self, priority: UBaseType) -> Self {
        if priority >= configMAX_PRIORITIES!() {
            warn!("Specified priority larger than system maximum priority, will be reduced.");
            info!(
                "MAX_PRIORITY is {}, but got {}",
                configMAX_PRIORITIES!() - 1,
                priority
            );
            self.task_priority = configMAX_PRIORITIES!() - 1;
        } else {
            self.task_priority = priority;
        }
        self
    }

    /// * Descrpition:
    ///
    ///  Internally, within the FreeRTOS implementation, tasks use two blocks of
    ///  memory.  The first block is used to hold the task's data structures.  The
    ///  second block is used by the task as its stack.  If a task is created using
    ///  xTaskCreate() then both blocks of memory are automatically dynamically
    ///  allocated inside the xTaskCreate() function.  (see
    ///  http://www.freertos.org/a00111.html).  If a task is created using
    ///  xTaskCreateStatic() then the application writer must provide the required
    ///  memory.  xTaskCreateStatic() therefore allows a task to be created without
    ///  using any dynamic memory allocation.
    ///
    ///  See xTaskCreateStatic() for a version that does not use any dynamic memory
    ///  allocation.
    ///
    ///  xTaskCreate() can only be used to create a task that has unrestricted
    ///  access to the entire microcontroller memory map.  Systems that include MPU
    ///  support can alternatively create an MPU constrained task using
    ///  xTaskCreateRestricted().
    ///
    /// * Implemented by: Fan Jinhao
    ///
    /// # Arguments:
    ///  `func` Pointer to the task entry function.  Tasks
    ///  must be implemented to never return (i.e. continuous loop).
    ///
    /// # Return:
    ///  `pdPASS` if the task was successfully created and added to a ready
    ///  list, otherwise an error code defined in the file projdefs.h
    ///
    ///
    pub fn initialise<F>(mut self, func: F) -> Result<TaskHandle, FreeRtosError>
    where
        F: FnOnce() -> () + Send + 'static,
    {
        let size_of_stacktype = std::mem::size_of::<StackType>();
        let stacksize_as_bytes = size_of_stacktype * self.task_stacksize as usize;
        trace!(
            "Initialising Task: {}, stack size: {} bytes",
            self.task_name,
            stacksize_as_bytes
        );

        // Return `Err` if malloc fails.
        let px_stack = port::port_malloc(stacksize_as_bytes)?;

        // A trick here. By changing raw pointer `px_stack` to StackType,
        // avoid using unsafe `*mut` as a struct field.
        // We don't lost any information here because raw pointers are actually addresses,
        // which can be stored as plain numbers.
        self.stack_pos = px_stack as StackType;
        trace!(
            "stack_pos for task {} is {}",
            self.task_name,
            self.stack_pos
        );

        let mut top_of_stack = self.stack_pos + self.task_stacksize as StackType - 1;
        top_of_stack = top_of_stack & portBYTE_ALIGNMENT_MASK as StackType;

        let f = Box::new(Box::new(func) as Box<FnBox()>); // Pass task function as a parameter.
        let param_ptr = &*f as *const _ as *mut _; // Convert to raw pointer.
        trace!(
            "Function ptr of {} is at {:X}",
            self.get_name(),
            param_ptr as u64
        );

        /* We use a wrapper function to call the task closure,
         * this is how freertos.rs approaches this problem, and is explained here:
         * https://stackoverflow.com/questions/32270030/how-do-i-convert-a-rust-closure-to-a-c-style-callback
         */
        let result =
            port::port_initialise_stack(top_of_stack as *mut _, Some(run_wrapper), param_ptr);
        match result {
            Ok(_) => {
                trace!("Stack initialisation succeeded");
                /* We MUST forget `f`, otherwise it will be freed at the end of this function.
                 * But we need to call `f` later in `run_wrapper`, which will lead to
                 * some unexpected behavior.
                 */
                mem::forget(f);
            }
            Err(e) => return Err(e),
        }

        /* Do a bunch of conditional initialisations. */
        #[cfg(feature = "configUSE_MUTEXES")]
        {
            self.mutexes_held = 0;
            self.base_priority = self.task_priority;
        }

        /* These list items were already initialised when `self` was created.
        list_initialise_item! (self.state_list_item);
        list_initialise_item! (self.event_list_item);
        */

        #[cfg(feature = "portCRITICAL_NESTING_IN_TCB")]
        {
            self.critical_nesting = 0;
        }

        #[cfg(feature = "configGENERATE_RUN_TIME_STATS")]
        {
            self.runtime_counter = 0;
        }

        #[cfg(feature = "config_USE_TASK_NOTIFICATIONS")]
        {
            self.notify_state = taskNOT_WAITING_NOTIFICATION;
            self.notified_value = 0;
        }

        // Create task handle.
        let sp = self.stack_pos;
        let handle = TaskHandle(Arc::new(RwLock::new(self)));
        // TODO: Change type of list_items.
        let state_list_item = handle.get_state_list_item();
        let event_list_item = handle.get_event_list_item();
        list::set_list_item_owner(&state_list_item, handle.clone());
        list::set_list_item_owner(&event_list_item, handle.clone());
        let item_value = (configMAX_PRIORITIES!() - handle.get_priority()) as TickType;
        list::set_list_item_value(&state_list_item, item_value);

        handle.add_new_task_to_ready_list()?;

        Ok(handle)
    }

    pub fn get_state_list_item(&self) -> ItemLink {
        Arc::clone(&self.state_list_item)
    }

    pub fn get_event_list_item(&self) -> ItemLink {
        Arc::clone(&self.event_list_item)
    }

    pub fn get_priority(&self) -> UBaseType {
        self.task_priority
    }

    pub fn set_priority(&mut self, new_priority: UBaseType) {
        self.task_priority = new_priority;
    }

    pub fn get_name(&self) -> String {
        self.task_name.clone()
    }

    #[cfg(feature = "configGENERATE_RUN_TIME_STATS")]
    pub fn get_run_time(&self) -> TickType {
        self.runtime_counter
    }

    #[cfg(feature = "configGENERATE_RUN_TIME_STATS")]
    pub fn set_run_time(&mut self, next_val: TickType) -> TickType {
        let prev_val: u32 = self.runtime_counter;
        self.runtime_counter = next_val;
        prev_val
    }

    #[cfg(feature = "INCLUDE_xTaskAbortDelay")]
    pub fn get_delay_aborted(&self) -> bool {
        self.delay_aborted
    }

    #[cfg(feature = "INCLUDE_xTaskAbortDelay")]
    pub fn set_delay_aborted(&mut self, next_val: bool) -> bool {
        let prev_val: bool = self.delay_aborted;
        self.delay_aborted = next_val;
        prev_val
    }

    #[cfg(feature = "configUSE_MUTEXES")]
    pub fn get_mutex_held_count(&self) -> UBaseType {
        self.mutexes_held
    }

    #[cfg(feature = "configUSE_MUTEXES")]
    pub fn set_mutex_held_count(&mut self, new_count: UBaseType) {
        self.mutexes_held = new_count;
    }

    pub fn get_base_priority(&self) -> UBaseType {
        self.base_priority
    }

    pub fn set_base_priority(&mut self, new_val: UBaseType) {
        self.base_priority = new_val
    }
}

impl PartialEq for TCB {
    fn eq(&self, other: &Self) -> bool {
        self.stack_pos == other.stack_pos
    }
}

/* Task call wrapper function. */
extern "C" fn run_wrapper(func_to_run: CVoidPointer) {
    info!(
        "Run_wrapper: The function is at position: {:X}",
        func_to_run as u64
    );
    unsafe {
        let func_to_run = Box::from_raw(func_to_run as *mut Box<FnBox() + 'static>);
        func_to_run();
        // TODO: Delete this wrapper task.
    }
}

// * Record the Highest ready priority
// * Usage:
// * Input: num
// * Output: None
#[macro_export]
macro_rules! record_ready_priority {
    ($priority:expr) => {{
        if $priority > get_top_ready_priority!() {
            set_top_ready_priority!($priority);
        }
    }};
}

/*
pub fn initialize_task_list () {
    for priority in (0..configMAX_PRIORITIES-1)	{
        list_initialise! ( READY_TASK_LIST [priority] );
    }

    list_initialise!( DELAY_TASK_LIST1 );
    list_initialise!( DELAY_TASK_LIST2 );
    list_initialise!( PENDING_READY_LIST );

    {
        #![cfg(INCLUDE_vTaskDelete)]
        list_initialise!( TASK_WATCHING_TERMINATION );
    }

    {
        #![cfg(INCLUDE_vTaskSuspend)]
        list_initialise!( SUSPEND_TASK_LIST );
    }

    /* Start with pxDelayedTaskList using list1 and the pxOverflowDelayedTaskList
       using list2. */
    DELAY_TASK_LIST = &DELAY_TASK_LIST1;
    OVERFLOW_DELAYED_TASK_LIST = &DELAY_TASK_LIST2;
}
*/

///  Type by which tasks are referenced.  For example, a call to xTaskCreate
///  returns (via a pointer parameter) an TaskHandle_t variable that can then
///  be used as a parameter to vTaskDelete to delete the task.
///  Since multiple `TaskHandle`s may refer to and own a same TCB at a time,
///  we wrapped TCB within a `tuple struct` using `Arc<RwLock<_>>`
///
/// * Implemented by: Fan Jinhao
///
#[derive(Clone)]
pub struct TaskHandle(Arc<RwLock<TCB>>);

impl PartialEq for TaskHandle {
    fn eq(&self, other: &Self) -> bool {
        *self.0.read().unwrap() == *other.0.read().unwrap()
    }
}

impl From<Weak<RwLock<TCB>>> for TaskHandle {
    fn from(weak_link: Weak<RwLock<TCB>>) -> Self {
        TaskHandle(
            weak_link
                .upgrade()
                .unwrap_or_else(|| panic!("Owner is not set")),
        )
    }
}

impl From<TaskHandle> for Weak<RwLock<TCB>> {
    fn from(task: TaskHandle) -> Self {
        Arc::downgrade(&task.0)
    }
}

impl TaskHandle {
    pub fn from_arc(arc: Arc<RwLock<TCB>>) -> Self {
        TaskHandle(arc)
    }

    /// Construct a TaskHandle with a TCB. */
    /// * Implemented by: Fan Jinhao.
    /// * C implementation:
    ///
    /// # Arguments
    /// * `tcb`: The TCB that we want to get TaskHandle from.
    ///
    /// # Return
    ///
    /// The created TaskHandle.
    pub fn from(tcb: TCB) -> Self {
        // TODO: Implement From.
        TaskHandle(Arc::new(RwLock::new(tcb)))
    }

    /* This function is for use in FFI. */
    pub fn as_raw(self) -> ffi::xTaskHandle {
        Arc::into_raw(self.0) as *mut _
    }

    pub fn get_priority(&self) -> UBaseType {
        /* Get the priority of a task.
         * Since this method is so frequently used, I used a funtion to do it.
         */
        self.0.read().unwrap().get_priority()
    }

    pub fn set_priority(&self, new_priority: UBaseType) {
        get_tcb_from_handle_mut!(self).set_priority(new_priority);
    }

    /// Place the task represented by pxTCB into the appropriate ready list for
    /// the task.  It is inserted at the end of the list.
    ///
    /// * Implemented by: Fan Jinhao.
    /// * C implementation:
    ///
    /// # Arguments
    ///
    ///
    /// # Return
    ///
    /// TODO
    pub fn add_task_to_ready_list(&self) -> Result<(), FreeRtosError> {
        let unwrapped_tcb = get_tcb_from_handle!(self);
        let priority = self.get_priority();

        traceMOVED_TASK_TO_READY_STATE!(&unwrapped_tcb);
        record_ready_priority!(priority);

        // let list_to_insert = (*READY_TASK_LISTS).write().unwrap();
        /* let list_to_insert = match list_to_insert {
            Ok(lists) => lists[unwrapped_tcb.task_priority as usize],
            Err(_) => {
                warn!("List was locked, read failed");
                return Err(FreeRtosError::DeadLocked);
            }
        };
        */
        // TODO: This line is WRONG! (just for test)
        // set_list_item_container!(unwrapped_tcb.state_list_item, list::ListName::READY_TASK_LISTS_1);
        list::list_insert_end(
            &READY_TASK_LISTS[priority as usize],
            Arc::clone(&unwrapped_tcb.state_list_item),
        );
        tracePOST_MOVED_TASK_TO_READY_STATE!(&unwrapped_tcb);
        Ok(())
    }

    /// Called after a new task has been created and initialised to place the task
    /// under the control of the scheduler.
    ///
    /// * Implemented by: Fan Jinhao.
    /// * C implementation:
    ///
    /// # Arguments
    ///
    ///
    /// # Return
    ///
    /// TODO
    fn add_new_task_to_ready_list(&self) -> Result<(), FreeRtosError> {
        let unwrapped_tcb = get_tcb_from_handle!(self);

        taskENTER_CRITICAL!();
        {
            // We don't need to initialise task lists any more.
            let n_o_t = get_current_number_of_tasks!() + 1;
            set_current_number_of_tasks!(n_o_t);
            /* CURRENT_TCB won't be None. See task_global.rs. */
            if task_global::CURRENT_TCB.read().unwrap().is_none() {
                set_current_task_handle!(self.clone());
                if get_current_number_of_tasks!() != 1 {
                    mtCOVERAGE_TEST_MARKER!(); // What happened?
                }
            } else {
                let unwrapped_cur = get_current_task_handle!();
                if !get_scheduler_running!() {
                    if unwrapped_cur.get_priority() <= unwrapped_tcb.task_priority {
                        /* If the scheduler is not already running, make this task the
                        current task if it is the highest priority task to be created
                        so far. */
                        set_current_task_handle!(self.clone());
                    } else {
                        mtCOVERAGE_TEST_MARKER!();
                    }
                }
            }
            set_task_number!(get_task_number!() + 1);
            traceTASK_CREATE!(self.clone());
            self.add_task_to_ready_list()?;
        }
        taskEXIT_CRITICAL!();
        if get_scheduler_running!() {
            let current_task_priority = get_current_task_handle!().get_priority();
            if current_task_priority < unwrapped_tcb.task_priority {
                taskYIELD_IF_USING_PREEMPTION!();
            } else {
                mtCOVERAGE_TEST_MARKER!();
            }
        } else {
            mtCOVERAGE_TEST_MARKER!();
        }

        Ok(())
    }

    pub fn get_event_list_item(&self) -> ItemLink {
        get_tcb_from_handle!(self).get_event_list_item()
    }

    pub fn get_state_list_item(&self) -> ItemLink {
        get_tcb_from_handle!(self).get_state_list_item()
    }

    pub fn get_name(&self) -> String {
        get_tcb_from_handle!(self).get_name()
    }

    #[cfg(feature = "configGENERATE_RUN_TIME_STATS")]
    pub fn get_run_time(&self) -> TickType {
        get_tcb_from_handle!(self).get_run_time()
    }

    #[cfg(feature = "configGENERATE_RUN_TIME_STATS")]
    pub fn set_run_time(&self, next_val: TickType) -> TickType {
        get_tcb_from_handle_mut!(self).set_run_time(next_val)
    }

    #[cfg(feature = "INCLUDE_xTaskAbortDelay")]
    pub fn get_delay_aborted(&self) -> bool {
        get_tcb_from_handle!(self).get_delay_aborted()
    }

    #[cfg(feature = "INCLUDE_xTaskAbortDelay")]
    pub fn set_delay_aborted(&self, next_val: bool) -> bool {
        get_tcb_from_handle_mut!(self).set_delay_aborted(next_val)
    }

    #[cfg(feature = "configUSE_MUTEXES")]
    pub fn get_mutex_held_count(&self) -> UBaseType {
        get_tcb_from_handle!(self).get_mutex_held_count()
    }

    #[cfg(feature = "configUSE_MUTEXES")]
    pub fn set_mutex_held_count(&self, new_count: UBaseType) {
        get_tcb_from_handle_mut!(self).set_mutex_held_count(new_count)
    }

    pub fn get_base_priority(&self) -> UBaseType {
        get_tcb_from_handle!(self).get_base_priority()
    }

    pub fn set_base_priority(&self, new_val: UBaseType) {
        get_tcb_from_handle_mut!(self).set_base_priority(new_val)
    }
}

#[macro_export]
macro_rules! get_tcb_from_handle {
    ($handle: expr) => {
        match $handle.0.try_read() {
            Ok(a) => a,
            Err(_) => {
                warn!("TCB was locked, read failed");
                panic!("Task handle locked!");
            }
        }
    };
}

#[macro_export]
macro_rules! get_tcb_from_handle_mut {
    ($handle: expr) => {
        match $handle.0.try_write() {
            Ok(a) => a,
            Err(_) => {
                warn!("TCB was locked, write failed");
                panic!("Task handle locked!");
            }
        }
    };
}

pub fn add_current_task_to_delayed_list(ticks_to_wait: TickType, can_block_indefinitely: bool) {
    /*
     * The currently executing task is entering the Blocked state.  Add the task to
     * either the current or the overflow delayed task list.
     */
    trace!("ADD");

    let unwrapped_cur = get_current_task_handle!();
    trace!("Remove succeeded");

    {
        #![cfg(feature = "INCLUDE_xTaskAbortDelay")]
        /* About to enter a delayed list, so ensure the ucDelayAborted flag is
        reset to pdFALSE so it can be detected as having been set to pdTRUE
        when the task leaves the Blocked state. */

        unwrapped_cur.set_delay_aborted(false);

        // NOTE by Fan Jinhao: Is this line necessary?
        // set_current_task_handle!(unwrapped_cur);
    }
    trace!("Abort succeeded");

    /* Remove the task from the ready list before adding it to the blocked list
    as the same list item is used for both lists. */
    if list::list_remove(unwrapped_cur.get_state_list_item()) == 0 {
        trace!("Returned 0");
        /* The current task must be in a ready list, so there is no need to
        check, and the port reset macro can be called directly. */
        portRESET_READY_PRIORITY!(unwrapped_cur.get_priority(), get_top_ready_priority!());
    } else {
        trace!("Returned not 0");
        mtCOVERAGE_TEST_MARKER!();
    }

    trace!("Remove succeeded");
    {
        #![cfg(feature = "INCLUDE_vTaskSuspend")]
        if ticks_to_wait == portMAX_DELAY && can_block_indefinitely {
            /* Add the task to the suspended task list instead of a delayed task
            list to ensure it is not woken by a timing event.  It will block
            indefinitely. */
            let cur_state_list_item = unwrapped_cur.get_state_list_item();
            list::list_insert_end(&SUSPENDED_TASK_LIST, cur_state_list_item);
        } else {
            /* Calculate the time at which the task should be woken if the event
            does not occur.  This may overflow but this doesn't matter, the
            kernel will manage it correctly. */
            let time_to_wake = get_tick_count!() + ticks_to_wait;

            /* The list item will be inserted in wake time order. */
            let cur_state_list_item = unwrapped_cur.get_state_list_item();
            list::set_list_item_value(&cur_state_list_item, time_to_wake);

            if time_to_wake < get_tick_count!() {
                /* Wake time has overflowed.  Place this item in the overflow
                list. */
                list::list_insert(&OVERFLOW_DELAYED_TASK_LIST, cur_state_list_item);
            } else {
                /* The wake time has not overflowed, so the current block list
                is used. */
                list::list_insert(&DELAYED_TASK_LIST, unwrapped_cur.get_state_list_item());

                /* If the task entering the blocked state was placed at the
                head of the list of blocked tasks then xNextTaskUnblockTime
                needs to be updated too. */
                if time_to_wake < get_next_task_unblock_time!() {
                    set_next_task_unblock_time!(time_to_wake);
                } else {
                    mtCOVERAGE_TEST_MARKER!();
                }
            }
        }
    }

    {
        #![cfg(not(feature = "INCLUDE_vTaskSuspend"))]
        /* Calculate the time at which the task should be woken if the event
        does not occur.  This may overflow but this doesn't matter, the kernel
        will manage it correctly. */
        let time_to_wake = get_tick_count!() + ticks_to_wait;

        let cur_state_list_item = unwrapped_cur.get_state_list_item();
        /* The list item will be inserted in wake time order. */
        list::set_list_item_value(&cur_state_list_item, time_to_wake);

        if time_to_wake < get_tick_count!() {
            /* Wake time has overflowed.  Place this item in the overflow list. */
            list::list_insert(&OVERFLOW_DELAYED_TASK_LIST, cur_state_list_item);
        } else {
            /* The wake time has not overflowed, so the current block list is used. */
            list::list_insert(&DELAYED_TASK_LIST, unwrapped_cur.get_state_list_item());

            /* If the task entering the blocked state was placed at the head of the
            list of blocked tasks then xNextTaskUnblockTime needs to be updated
            too. */
            if time_to_wake < get_next_task_unblock_time!() {
                set_next_task_unblock_time!(time_to_wake);
            } else {
                mtCOVERAGE_TEST_MARKER!();
            }
        }

        /* Avoid compiler warning when INCLUDE_vTaskSuspend is not 1. */
        // ( void ) xCanBlockIndefinitely;
    }

    trace!("Place succeeded");
}

pub fn reset_next_task_unblock_time() {
    if list_is_empty(&DELAYED_TASK_LIST) {
        /* The new current delayed list is empty.  Set xNextTaskUnblockTime to
        the maximum possible value so it is	extremely unlikely that the
        if( xTickCount >= xNextTaskUnblockTime ) test will pass until
        there is an item in the delayed list. */
        set_next_task_unblock_time!(portMAX_DELAY);
    } else {
        /* The new current delayed list is not empty, get the value of
        the item at the head of the delayed list.  This is the time at
        which the task at the head of the delayed list should be removed
        from the Blocked state. */
        let mut temp = get_owner_of_head_entry(&DELAYED_TASK_LIST);
        set_next_task_unblock_time!(get_list_item_value(&temp.get_state_list_item()));
    }
}

#[macro_export]
macro_rules! get_handle_from_option {
    ($option: expr) => {
        match $option {
            Some(handle) => handle,
            None => get_current_task_handle!(),
        }
    };
}

///  INCLUDE_vTaskDelete must be defined as 1 for this function to be available.
///  See the configuration section for more information.
///
///  Remove a task from the RTOS real time kernel's management.  The task being
///  deleted will be removed from all ready, blocked, suspended and event lists.
///
///  NOTE:  The idle task is responsible for freeing the kernel allocated
///  memory from tasks that have been deleted.  It is therefore important that
///  the idle task is not starved of microcontroller processing time if your
///  application makes any calls to vTaskDelete ().  Memory allocated by the
///  task code is not automatically freed, and should be freed before the task
///  is deleted.
///
///  See the demo application file death.c for sample code that utilises
///  vTaskDelete ().
///
///
/// * Implemented by: Huang Yeqi
///
/// # Arguments:
///  `task_to_delete` The handle of the task to be deleted.  Passing NULL will
///  cause the calling task to be deleted.
///
/// # Return:
///
#[cfg(feature = "INCLUDE_vTaskDelete")]
pub fn task_delete(task_to_delete: Option<TaskHandle>) {
    /* If null is passed in here then it is the calling task that is
    being deleted. */
    let pxtcb = get_handle_from_option!(task_to_delete);

    taskENTER_CRITICAL!();
    {
        /* Remove task from the ready list. */
        if list::list_remove(pxtcb.get_state_list_item()) == 0 {
            taskRESET_READY_PRIORITY!(pxtcb.get_priority());
        } else {
            mtCOVERAGE_TEST_MARKER!();
        }

        /* Is the task waiting on an event also? */
        if list::get_list_item_container(&pxtcb.get_event_list_item()).is_some() {
            list::list_remove(pxtcb.get_event_list_item());
        } else {
            mtCOVERAGE_TEST_MARKER!();
        }

        /* Increment the uxTaskNumber also so kernel aware debuggers can
        detect that the task lists need re-generating.  This is done before
        portPRE_TASK_DELETE_HOOK() as in the Windows port that macro will
        not return. */
        set_task_number!(get_task_number!() + 1);

        if pxtcb == get_current_task_handle!() {
            /* A task is deleting itself.  This cannot complete within the
            task itself, as a context switch to another task is required.
            Place the task in the termination list.  The idle task will
            check the termination list and free up any memory allocated by
            the scheduler for the TCB and stack of the deleted task. */
            list::list_insert_end(&TASKS_WAITING_TERMINATION, pxtcb.get_state_list_item());

            /* Increment the ucTasksDeleted variable so the idle task knows
            there is a task that has been deleted and that it should therefore
            check the xTasksWaitingTermination list. */
            set_deleted_tasks_waiting_clean_up!(get_deleted_tasks_waiting_clean_up!() + 1);

            /* The pre-delete hook is primarily for the Windows simulator,
            in which Windows specific clean up operations are performed,
            after which it is not possible to yield away from this task -
            hence xYieldPending is used to latch that a context switch is
            required. */
            portPRE_TASK_DELETE_HOOK!(pxtcb, get_yield_pending!());
        } else {
            set_current_number_of_tasks!(get_current_number_of_tasks!() - 1);

            let stack_pos = get_tcb_from_handle!(pxtcb).stack_pos;
            /* This call is required specifically for the TriCore port.  It must be
            above the vPortFree() calls.  The call is also used by ports/demos that
            want to allocate and clean RAM statically. */
            port::port_free(stack_pos as *mut _);

            /* Reset the next expected unblock time in case it referred to
            the task that has just been deleted. */
            reset_next_task_unblock_time();
        }
        // FIXME
        //traceTASK_DELETE!(task_to_delete);
    }
    taskEXIT_CRITICAL!();

    /* Force a reschedule if it is the currently running task that has just
    been deleted. */
    if get_scheduler_suspended!() > 0 {
        if pxtcb == get_current_task_handle!() {
            assert!(get_scheduler_suspended!() == 0);
            portYIELD_WITHIN_API!();
        } else {
            mtCOVERAGE_TEST_MARKER!();
        }
    }
}

///  INCLUDE_vTaskSuspend must be defined as 1 for this function to be available.
///  See the configuration section for more information.
///
///  Suspend any task.  When suspended a task will never get any microcontroller
///  processing time, no matter what its priority.
///
///  Calls to vTaskSuspend are not accumulative -
///  i.e. calling vTaskSuspend () twice on the same task still only requires one
///  call to vTaskResume () to ready the suspended task.
///
///
/// * Implemented by: Huang Yeqi
///
/// # Arguments:
///  `task_to_suspend` Handle to the task being suspended.  Passing a NULL
///  handle will cause the calling task to be suspended.
///
/// # Return:
///
#[cfg(feature = "INCLUDE_vTaskSuspend")]
pub fn suspend_task(task_to_suspend: TaskHandle) {
    trace!("suspend_task called!");
    /*
     * origin: If null is passed in here then it is the running task that is
     * being suspended. In our implement, you can just pass the TaskHandle of the current task
     */
    let mut unwrapped_tcb = get_tcb_from_handle!(task_to_suspend);
    taskENTER_CRITICAL!();
    {
        traceTASK_SUSPEND!(&unwrapped_tcb);

        /* Remove task from the ready/delayed list and place in the
        suspended list. */
        if list_remove(unwrapped_tcb.get_state_list_item()) == 0 {
            taskRESET_READY_PRIORITY!(unwrapped_tcb.get_priority());
        } else {
            mtCOVERAGE_TEST_MARKER!();
        }

        /* Is the task waiting on an event also? */
        if get_list_item_container(&unwrapped_tcb.get_event_list_item()).is_some() {
            list_remove(unwrapped_tcb.get_event_list_item());
        } else {
            mtCOVERAGE_TEST_MARKER!();
        }
        list_insert_end(&SUSPENDED_TASK_LIST, unwrapped_tcb.get_state_list_item());
    }
    taskEXIT_CRITICAL!();

    if get_scheduler_running!() {
        /* Reset the next expected unblock time in case it referred to the
        task that is now in the Suspended state. */
        taskENTER_CRITICAL!();
        {
            reset_next_task_unblock_time();
        }
        taskEXIT_CRITICAL!();
    } else {
        mtCOVERAGE_TEST_MARKER!();
    }

    if task_to_suspend == get_current_task_handle!() {
        if get_scheduler_running!() {
            /* The current task has just been suspended. */
            assert!(get_scheduler_suspended!() == 0);
            portYIELD_WITHIN_API!();
        } else {
            /* The scheduler is not running, but the task that was pointed
            to by pxCurrentTCB has just been suspended and pxCurrentTCB
            must be adjusted to point to a different task. */
            if current_list_length(&SUSPENDED_TASK_LIST) != get_current_number_of_tasks!() {
                task_switch_context();
            }
            //TODO: comprehend the implement of cuurrent_tcb
            /* But is the Source code, if the length == current number, it means no other tasks are ready, so set pxCurrentTCB back to
            NULL so when the next task is created pxCurrentTCB will
            be set to point to it no matter what its relative priority
            is. */
        }
    } else {
        mtCOVERAGE_TEST_MARKER!();
    }
}

#[cfg(feature = "INCLUDE_vTaskSuspend")]
pub fn task_is_tasksuspended(xtask: &TaskHandle) -> bool {
    let mut xreturn: bool = false;
    let tcb = get_tcb_from_handle!(xtask);
    /* Accesses xPendingReadyList so must be called from a critical
    section. */

    /* It does not make sense to check if the calling task is suspended. */
    //assert!( xtask );

    /* Is the task being resumed actually in the suspended list? */
    if is_contained_within(&SUSPENDED_TASK_LIST, &tcb.get_state_list_item()) {
        /* Has the task already been resumed from within an ISR? */
        if !is_contained_within(&PENDING_READY_LIST, &tcb.get_event_list_item()) {
            /* Is it in the suspended list because it is in the	Suspended
            state, or because is is blocked with no timeout? */
            if get_list_item_container(&tcb.get_event_list_item()).is_none() {
                xreturn = true;
            } else {
                mtCOVERAGE_TEST_MARKER!();
            }
        } else {
            mtCOVERAGE_TEST_MARKER!();
        }
    } else {
        mtCOVERAGE_TEST_MARKER!();
    }

    xreturn
}

///  INCLUDE_vTaskSuspend must be defined as 1 for this function to be available.
///  See the configuration section for more information.
///
///  Resumes a suspended task.
///
///  A task that has been suspended by one or more calls to vTaskSuspend ()
///  will be made available for running again by a single call to
///  vTaskResume ().
///
///
/// * Implemented by: Huang Yeqi
///
/// # Arguments:
///  `task_to_resume` Handle to the task being readied.
///
/// # Return:
///
#[cfg(feature = "INCLUDE_vTaskSuspend")]
pub fn resume_task(task_to_resume: TaskHandle) {
    trace!("resume task called!");
    let mut unwrapped_tcb = get_tcb_from_handle!(task_to_resume);

    if task_to_resume != get_current_task_handle!() {
        taskENTER_CRITICAL!();
        {
            if task_is_tasksuspended(&task_to_resume) {
                traceTASK_RESUME!(&unwrapped_tcb);

                /* As we are in a critical section we can access the ready
                lists even if the scheduler is suspended. */
                list_remove(unwrapped_tcb.get_state_list_item());
                task_to_resume.add_task_to_ready_list();

                let current_task_priority = get_current_task_handle!().get_priority();
                /* We may have just resumed a higher priority task. */
                if unwrapped_tcb.get_priority() >= current_task_priority {
                    /* This yield may not cause the task just resumed to run,
                    but will leave the lists in the correct state for the
                    next yield. */
                    taskYIELD_IF_USING_PREEMPTION!();
                } else {
                    mtCOVERAGE_TEST_MARKER!();
                }
            } else {
                mtCOVERAGE_TEST_MARKER!();
            }
        }
        taskEXIT_CRITICAL!();
    } else {
        mtCOVERAGE_TEST_MARKER!();
    }
}
use crate::list::ListLink;
use crate::port::{BaseType, TickType, UBaseType};
use crate::task_control::TaskHandle;
use crate::*;
use std::sync::RwLock;

/* Some global variables. */
pub static mut TICK_COUNT: TickType = 0;
pub static mut TOP_READY_PRIORITY: UBaseType = 0;
pub static mut PENDED_TICKS: UBaseType = 0;
pub static mut SCHEDULER_RUNNING: bool = false;
pub static mut YIELD_PENDING: bool = false;
pub static mut NUM_OF_OVERFLOWS: BaseType = 0;
pub static mut TASK_NUMBER: UBaseType = 0;
pub static mut NEXT_TASK_UNBLOCK_TIME: TickType = 0;
pub static mut CURRENT_NUMBER_OF_TASKS: UBaseType = 0;

/* GLOBAL TASK LISTS ARE CHANGED TO INTEGERS, WHICH ARE THEIR IDS. */

/* Current_TCB and global task lists. */
lazy_static! {
    /* Initialise CURRENT_TCB as early as it is declared rather than when the scheduler starts running.
     * This isn't reasonable actually, but avoided the complexity of using an additional Option<>.
     * Use RwLock to wrap TaskHandle because sometimes we need to change CURRENT_TCB.
     * We use setter and getter to modify CURRENT_TCB, they are defined at the end of this file.
     */
    pub static ref CURRENT_TCB: RwLock<Option<TaskHandle>> = RwLock::new(None);
    pub static ref READY_TASK_LISTS: [ListLink; configMAX_PRIORITIES!()] = Default::default();

    /* Delayed tasks (two lists are used - one for delays that have overflowed the current tick count.
    */
    // Points to the delayed task list currently being used.
    pub static ref DELAYED_TASK_LIST: ListLink = Default::default();

    /* Points to the delayed task list currently being used
     * to hold tasks that have overflowed the current tick count.
     */
    pub static ref OVERFLOW_DELAYED_TASK_LIST: ListLink = Default::default();

    /* Tasks that have been readied while the scheduler was suspended.
     * They will be moved to the ready list when the scheduler is resumed.
     */
    pub static ref PENDING_READY_LIST: ListLink = Default::default();
}

#[cfg(feature = "INCLUDE_vTaskDelete")]
lazy_static! {
    // Tasks that have been deleted - but their memory not yet freed.
    pub static ref TASKS_WAITING_TERMINATION: ListLink = Default::default();
}

#[cfg(feature = "INCLUDE_vTaskSuspend")]
lazy_static! {
    // Tasks that are currently suspended.
    pub static ref SUSPENDED_TASK_LIST: ListLink = Default::default();
}
/* ------------------ End global lists ------------------- */

/* Context switches are held pending while the scheduler is suspended.  Also,
interrupts must not manipulate the xStateListItem of a TCB, or any of the
lists the xStateListItem can be referenced from, if the scheduler is suspended.
*/
pub static mut SCHEDULER_SUSPENDED: UBaseType = 0;

/*< Holds the value of a timer/counter the last time a task was switched in. */
#[cfg(feature = "configGENERATE_RUN_TIME_STATS")]
pub static mut TASK_SWITCHED_IN_TIME: u32 = 0;

/*< Holds the total amount of execution time as defined by the run time counter clock. */
#[cfg(feature = "configGENERATE_RUN_TIME_STATS")]
pub static mut TOTAL_RUN_TIME: u32 = 0;

#[cfg(feature = "INCLUDE_vTaskDelete")]
pub static mut DELETED_TASKS_WAITING_CLEAN_UP: UBaseType = 0;

/* Setters and getters of the above global variables to avoid redundancy of unsafe blocks. */
#[macro_export]
macro_rules! set_scheduler_suspended {
    ($next_val: expr) => {
        unsafe {
            trace!("SCHEDULER_SUSPENDED was set to {}", $next_val);
            crate::task_global::SCHEDULER_SUSPENDED = $next_val;
        }
    };
}

#[macro_export]
macro_rules! get_scheduler_suspended {
    () => {
        unsafe { crate::task_global::SCHEDULER_SUSPENDED }
    };
}

#[macro_export]
macro_rules! set_deleted_tasks_waiting_clean_up {
    ($next_val: expr) => {
        unsafe {
            trace!("DELETED_TASKS_WAITING_CLEAN_UP was set to {}", $next_val);
            crate::task_global::DELETED_TASKS_WAITING_CLEAN_UP = $next_val;
        }
    };
}

#[macro_export]
macro_rules! get_deleted_tasks_waiting_clean_up {
    () => {
        unsafe { crate::task_global::DELETED_TASKS_WAITING_CLEAN_UP }
    };
}

#[macro_export]
macro_rules! get_top_ready_priority {
    () => {
        unsafe { crate::task_global::TOP_READY_PRIORITY }
    };
}

#[macro_export]
macro_rules! set_top_ready_priority {
    ($new_top_ready_priority: expr) => {
        unsafe {
            trace!("TOP_READY_PRIORITY was set to {}", $new_top_ready_priority);
            crate::task_global::TOP_READY_PRIORITY = $new_top_ready_priority;
        }
    };
}

#[macro_export]
macro_rules! set_pended_ticks {
    ($next_val: expr) => {
        unsafe {
            trace!("PENDED_TICKS was set to {}", $next_val);
            crate::task_global::PENDED_TICKS = $next_val
        }
    };
}

#[macro_export]
macro_rules! get_pended_ticks {
    () => {
        unsafe { crate::task_global::PENDED_TICKS }
    };
}

#[macro_export]
macro_rules! set_task_number {
    ($next_val: expr) => {
        unsafe {
            trace!("TASK_NUMBER was set to {}", $next_val);
            crate::task_global::TASK_NUMBER = $next_val
        }
    };
}

#[macro_export]
macro_rules! get_task_number {
    () => {
        unsafe { crate::task_global::TASK_NUMBER }
    };
}

#[macro_export]
macro_rules! get_yield_pending {
    () => {
        unsafe { crate::task_global::YIELD_PENDING }
    };
}

#[macro_export]
macro_rules! set_yield_pending {
    ($true_or_flase: expr) => {
        unsafe {
            trace!("YIELD_PENDING was set to {}", $true_or_flase);
            crate::task_global::YIELD_PENDING = $true_or_flase;
        }
    };
}

#[macro_export]
macro_rules! set_current_number_of_tasks {
    ($next_val: expr) => {
        unsafe {
            trace!("CURRENT_NUMBER_OF_TASKS was set to {}", $next_val);
            crate::task_global::CURRENT_NUMBER_OF_TASKS = $next_val;
        }
    };
}

#[macro_export]
macro_rules! get_current_number_of_tasks {
    () => {
        unsafe { crate::task_global::CURRENT_NUMBER_OF_TASKS }
    };
}

#[macro_export]
macro_rules! set_scheduler_running {
    ($true_or_flase: expr) => {
        unsafe {
            trace!("SCHEDULER_RUNNING was set to {}", $true_or_flase);
            crate::task_global::SCHEDULER_RUNNING = $true_or_flase
        }
    };
}

#[macro_export]
macro_rules! get_scheduler_running {
    () => {
        unsafe { crate::task_global::SCHEDULER_RUNNING }
    };
}

#[macro_export]
macro_rules! get_next_task_unblock_time {
    () => {
        unsafe { crate::task_global::NEXT_TASK_UNBLOCK_TIME }
    };
}

#[macro_export]
macro_rules! set_next_task_unblock_time {
    ($new_time: expr) => {
        unsafe {
            trace!("NEXT_TASK_UNBLOCK_TIME was set to {}", $new_time);
            crate::task_global::NEXT_TASK_UNBLOCK_TIME = $new_time;
        }
    };
}

#[macro_export]
macro_rules! get_tick_count {
    () => {
        unsafe { crate::task_global::TICK_COUNT }
    };
}

#[macro_export]
macro_rules! set_tick_count {
    ($next_tick_count: expr) => {
        unsafe {
            trace!("TICK_COUNT was set to {}", $next_tick_count);
            crate::task_global::TICK_COUNT = $next_tick_count;
        }
    };
}

#[macro_export]
macro_rules! get_num_of_overflows {
    () => {
        unsafe { crate::task_global::NUM_OF_OVERFLOWS }
    };
}

#[macro_export]
macro_rules! set_num_of_overflows {
    ($next_tick_count: expr) => {
        unsafe {
            trace!("NUM_OF_OVERFLOWS was set to {}", $next_tick_count);
            crate::task_global::NUM_OF_OVERFLOWS = $next_tick_count;
        }
    };
}

#[macro_export]
#[cfg(feature = "configGENERATE_RUN_TIME_STATS")]
macro_rules! set_total_run_time {
    ($next_val: expr) => {
        unsafe {
            trace!("TOTAL_RUN_TIME was set to {}", $next_val);
            TOTAL_RUN_TIME = $next_val
        }
    };
}

#[macro_export]
#[cfg(feature = "configGENERATE_RUN_TIME_STATS")]
macro_rules! set_task_switch_in_time {
    ($next_val: expr) => {
        unsafe {
            trace!("TASK_SWITCHED_IN_TIME was set to {}", $next_val);
            TASK_SWITCHED_IN_TIME = $next_val
        }
    };
}

#[macro_export]
#[cfg(feature = "configGENERATE_RUN_TIME_STATS")]
macro_rules! get_total_run_time {
    () => {
        unsafe { TOTAL_RUN_TIME }
    };
}

#[macro_export]
#[cfg(feature = "configGENERATE_RUN_TIME_STATS")]
macro_rules! get_task_switch_in_time {
    () => {
        unsafe { TASK_SWITCHED_IN_TIME }
    };
}

#[macro_export]
macro_rules! get_current_task_handle_wrapped {
    () => {
        // NOTE: This macro WILL be deprecated. So please avoid using this macro.
        crate::task_global::CURRENT_TCB.read().unwrap().as_ref()
    };
}

#[macro_export]
macro_rules! get_current_task_handle {
    () => {
        crate::task_global::CURRENT_TCB
            .read()
            .unwrap()
            .as_ref()
            .unwrap()
            .clone()
    };
}

#[macro_export]
macro_rules! set_current_task_handle {
    ($cloned_new_task: expr) => {
        trace!("CURRENT_TCB changed!");
        *(crate::task_global::CURRENT_TCB).write().unwrap() = Some($cloned_new_task)
    };
}

#[macro_export]
macro_rules! get_current_task_priority {
    () => {
        get_current_task_handle!().get_priority()
    };
}

#[cfg(feature = "INCLUDE_xTaskAbortDelay")]
#[macro_export]
macro_rules! get_current_task_delay_aborted {
    () => {
        get_current_task_handle!().get_delay_aborted()
    };
}
/* ---------- End of global variable setters and getters -----------*/

#[macro_export]
macro_rules! taskCHECK_FOR_STACK_OVERFLOW {
    () => {
        // This macro does nothing.
    };
}

#[macro_export]
macro_rules! switch_delayed_lists {
    () => {
        /* pxDelayedTaskList and pxOverflowDelayedTaskList are switched when the tick
        count overflows. */
        // TODO: tasks.c 239
        unsafe {
            let mut delayed = DELAYED_TASK_LIST.write().unwrap();
            let mut overflowed = OVERFLOW_DELAYED_TASK_LIST.write().unwrap();
            let tmp = (*delayed).clone();
            *delayed = (*overflowed).clone();
            *overflowed = tmp;
        }
    };
}
use crate::list;
use crate::list::ListLink;
use crate::port::*;
// use crate::kernel::*;
use crate::projdefs::pdFALSE;
use crate::task_control::*;
use crate::task_global::*;
use crate::*;

/*
 * The item value of the event list item is normally used to hold the priority of
 * the task to which it belongs (coded to allow it to be held in reverse
 * priority order).  However, it is occasionally borrowed for other purposes.  It
 * is important its value is not updated due to a task priority change while it is
 * being used for another purpose.  The following bit definition is used to inform
 * the scheduler that the value should not be changed - in which case it is the
 * responsibility of whichever module is using the value to ensure it gets set back
 * to its original value when it is released.
 */
#[cfg(feature = "configUSE_16_BIT_TICKS")]
pub const taskEVENT_LIST_ITEM_VALUE_IN_USE: TickType = 0x8000;
#[cfg(not(feature = "configUSE_16_BIT_TICKS"))]
pub const taskEVENT_LIST_ITEM_VALUE_IN_USE: TickType = 0x80000000;

pub fn task_remove_from_event_list(event_list: &ListLink) -> bool {
    let unblocked_tcb = list::get_owner_of_head_entry(event_list);
    // configASSERT( unblocked_tcb );
    let mut xreturn: bool = false;

    list::list_remove(unblocked_tcb.get_event_list_item());

    if get_scheduler_suspended!() == pdFALSE as UBaseType {
        list::list_remove(unblocked_tcb.get_state_list_item());
        unblocked_tcb.add_task_to_ready_list().unwrap();
    } else {
        list::list_insert_end(&PENDING_READY_LIST, unblocked_tcb.get_event_list_item());
    }

    if unblocked_tcb.get_priority() > get_current_task_priority!() {
        /* Return true if the task removed from the event list has a higher
        priority than the calling task.  This allows the calling task to know if
        it should force a context switch now. */
        xreturn = true;

        /* Mark that a yield is pending in case the user is not using the
        "xHigherPriorityTaskWoken" parameter to an ISR safe FreeRTOS function. */
        set_yield_pending!(true);
    } else {
        xreturn = false;
    }

    {
        #![cfg(feature = "configUSE_TICKLESS_IDLE")]
        reset_next_task_unblock_time();
    }

    trace!("xreturn is {}", xreturn);
    xreturn
}

pub fn task_missed_yield() {
    set_yield_pending!(false);
}

/*
 * Used internally only.
 */
#[derive(Debug, Default)]
pub struct time_out {
    overflow_count: BaseType,
    time_on_entering: TickType,
}

pub fn task_set_time_out_state(pxtimeout: &mut time_out) {
    // assert! ( pxtimeout );
    pxtimeout.overflow_count = get_num_of_overflows!();
    pxtimeout.time_on_entering = get_tick_count!();
}

pub fn task_check_for_timeout(pxtimeout: &mut time_out, ticks_to_wait: &mut TickType) -> bool {
    trace!("time_out is {:?}", pxtimeout);
    trace!("ticks_to_wait is {}", ticks_to_wait);
    let mut xreturn: bool = false;
    // assert! (pxtimeout);
    // assert! (ticks_to_wait);

    taskENTER_CRITICAL!();
    {
        let const_tick_count: TickType = get_tick_count!();
        trace!("Tick_count is {}", const_tick_count);
        let unwrapped_cur = get_current_task_handle!();
        let mut cfglock1 = false;
        let mut cfglock2 = false;

        {
            #![cfg(feature = "INCLUDE_xTaskAbortDelay")]
            cfglock1 = true;
        }

        {
            #![cfg(feature = "INCLUDE_vTaskSuspend")]
            cfglock2 = true;
        }

        if cfglock1 && unwrapped_cur.get_delay_aborted() {
            unwrapped_cur.set_delay_aborted(false);
            xreturn = true;
        }

        if cfglock2 && *ticks_to_wait == portMAX_DELAY {
            xreturn = false;
        }

        if get_num_of_overflows!() != pxtimeout.overflow_count
            && const_tick_count >= pxtimeout.time_on_entering
        {
            trace!("IF");
            xreturn = true;
        } else if const_tick_count - pxtimeout.time_on_entering < *ticks_to_wait {
            trace!("ELSE IF");
            *ticks_to_wait -= const_tick_count - pxtimeout.time_on_entering;
            task_set_time_out_state(pxtimeout);
            xreturn = false;
        } else {
            trace!("ELSE");
            xreturn = true;
        }
    }
    taskEXIT_CRITICAL!();

    xreturn
}

pub fn task_place_on_event_list(event_list: &ListLink, ticks_to_wait: TickType) {
    // assert! ( event_list );

    /* THIS FUNCTION MUST BE CALLED WITH EITHER INTERRUPTS DISABLED OR THE
    SCHEDULER SUSPENDED AND THE QUEUE BEING ACCESSED LOCKED. */

    /* Place the event list item of the TCB in the appropriate event list.
    This is placed in the list in priority order so the highest priority task
    is the first to be woken by the event.  The queue that contains the event
    list is locked, preventing simultaneous access from interrupts. */

    let unwrapped_cur = get_current_task_handle!();
    trace!("INSERT");
    list::list_insert(event_list, unwrapped_cur.get_event_list_item());
    trace!("INSERT SUCCEEDED");

    add_current_task_to_delayed_list(ticks_to_wait, true);
    trace!("ADD SUCCEEDED");
}

#[cfg(feature = "configUSE_MUTEXES")]
pub fn task_increment_mutex_held_count() -> Option<TaskHandle> {
    /* If xSemaphoreCreateMutex() is called before any tasks have been created
    then pxCurrentTCB will be NULL. */
    match get_current_task_handle_wrapped!() {
        Some(current_task) => {
            let new_val = current_task.get_mutex_held_count() + 1;
            current_task.set_mutex_held_count(new_val);
            Some(current_task.clone())
        }
        None => None,
    }
}

#[cfg(feature = "configUSE_MUTEXES")]
pub fn task_priority_inherit(mutex_holder: Option<TaskHandle>) {
    /* NOTE by Fan Jinhao: Maybe mutex_holder should be `&Option<TaskHandle>`.
     * But I'll leave it for now.
     */
    trace!("Enter function 'task_priority_inherit'");
    /* If the mutex was given back by an interrupt while the queue was
    locked then the mutex holder might now be NULL. */
    if mutex_holder.is_some() {
        trace!("Mutex holder exists!");
        let task = mutex_holder.unwrap();
        /* If the holder of the mutex has a priority below the priority of
        the task attempting to obtain the mutex then it will temporarily
        inherit the priority of the task attempting to obtain the mutex. */
        let current_task_priority = get_current_task_priority!();
        let this_task_priority = task.get_priority();

        if this_task_priority < current_task_priority {
            /* Adjust the mutex holder state to account for its new
            priority.  Only reset the event list item value if the value is
            not being used for anything else. */
            trace!("change priority!");
            let event_list_item = task.get_event_list_item();
            if (list::get_list_item_value(&event_list_item) & taskEVENT_LIST_ITEM_VALUE_IN_USE) == 0
            {
                let new_item_val = (configMAX_PRIORITIES!() - current_task_priority) as TickType;
                list::set_list_item_value(&event_list_item, new_item_val);
            } else {
                mtCOVERAGE_TEST_MARKER!();
            }

            /* If the task being modified is in the ready state it will need
            to be moved into a new list. */
            let state_list_item = task.get_state_list_item();
            if list::is_contained_within(
                &READY_TASK_LISTS[this_task_priority as usize],
                &state_list_item,
            ) {
                if list::list_remove(state_list_item) == 0 {
                    taskRESET_READY_PRIORITY!(this_task_priority);
                } else {
                    mtCOVERAGE_TEST_MARKER!();
                }

                /* Inherit the priority before being moved into the new list. */
                task.set_priority(current_task_priority);
                task.add_task_to_ready_list().unwrap();
            }
        } else {
            mtCOVERAGE_TEST_MARKER!();
        }
    } else {
        mtCOVERAGE_TEST_MARKER!();
    }
}

#[cfg(feature = "configUSE_MUTEXES")]
pub fn task_priority_disinherit(mutex_holder: Option<TaskHandle>) -> bool {
    /* NOTE by Fan Jinhao: Maybe mutex_holder should be `&Option<TaskHandle>`.
     * But I'll leave it for now.
     */
    let mut ret_val: bool = false;
    trace!("Enter function 'task_priority_disinherit'");
    if let Some(task) = mutex_holder {
        /* A task can only have an inherited priority if it holds the mutex.
        If the mutex is held by a task then it cannot be given from an
        interrupt, and if a mutex is given by the holding task then it must
        be the running state task. */

        assert!(task == get_current_task_handle!());

        let mutex_held = task.get_mutex_held_count();
        assert!(mutex_held > 0);
        let mutex_held = mutex_held - 1;
        task.set_mutex_held_count(mutex_held);

        /* Has the holder of the mutex inherited the priority of another
        task? */
        let this_task_priority = task.get_priority();
        let this_task_base_priority = task.get_base_priority();
        if this_task_priority != this_task_base_priority {
            /* Only disinherit if no other mutexes are held. */
            if mutex_held == 0 {
                let state_list_item = task.get_state_list_item();

                /* A task can only have an inherited priority if it holds
                the mutex.  If the mutex is held by a task then it cannot be
                given from an interrupt, and if a mutex is given by the
                holding	task then it must be the running state task.  Remove
                the	holding task from the ready	list. */
                if list::list_remove(state_list_item) == 0 {
                    taskRESET_READY_PRIORITY!(this_task_priority);
                } else {
                    mtCOVERAGE_TEST_MARKER!();
                }

                /* Disinherit the priority before adding the task into the
                new	ready list. */
                traceTASK_PRIORITY_DISINHERIT!(&task, this_task_base_priority);
                task.set_priority(this_task_base_priority);

                /* Reset the event list item value.  It cannot be in use for
                any other purpose if this task is running, and it must be
                running to give back the mutex. */
                let new_item_val = (configMAX_PRIORITIES!() - this_task_priority) as TickType;
                list::set_list_item_value(&task.get_event_list_item(), new_item_val);
                task.add_task_to_ready_list().unwrap();

                /* Return true to indicate that a context switch is required.
                This is only actually required in the corner case whereby
                multiple mutexes were held and the mutexes were given back
                in an order different to that in which they were taken.
                If a context switch did not occur when the first mutex was
                returned, even if a task was waiting on it, then a context
                switch should occur when the last mutex is returned whether
                a task is waiting on it or not. */
                ret_val = true;
            } else {
                mtCOVERAGE_TEST_MARKER!();
            }
        } else {
            mtCOVERAGE_TEST_MARKER!();
        }
    } else {
        mtCOVERAGE_TEST_MARKER!();
    }

    ret_val
}
use crate::kernel::*;
use crate::list::*;
use crate::port::*;
use crate::task_control::*;
use crate::*;
use std::ffi::*;
use std::mem::*;

///  Delay a task for a given number of ticks.  The actual time that the
///  task remains blocked depends on the tick rate.  The constant
///  portTICK_PERIOD_MS can be used to calculate real time from the tick
///  rate - with the resolution of one tick period.
///
///  INCLUDE_vTaskDelay must be defined as 1 for this function to be available.
///  See the configuration section for more information.
///
///
///  vTaskDelay() specifies a time at which the task wishes to unblock relative to
///  the time at which vTaskDelay() is called.  For example, specifying a block
///  period of 100 ticks will cause the task to unblock 100 ticks after
///  vTaskDelay() is called.  vTaskDelay() does not therefore provide a good method
///  of controlling the frequency of a periodic task as the path taken through the
///  code, as well as other task and interrupt activity, will effect the frequency
///  at which vTaskDelay() gets called and therefore the time at which the task
///  next executes.  See vTaskDelayUntil() for an alternative API function designed
///  to facilitate fixed frequency execution.  It does this by specifying an
///  absolute time (rather than a relative time) at which the calling task should
///  unblock.
///
/// * Implemented by: Fan Jinhao
///
/// # Arguments:
///  `ticks_to_delay` The amount of time, in tick periods, that the calling task should block.
///
/// * Return:
///

pub fn task_delay(ticks_to_delay: TickType) {
    let mut already_yielded = false;

    if ticks_to_delay > 0 {
        assert!(get_scheduler_suspended!() == 0);

        task_suspend_all();
        {
            traceTASK_DELAY!();
            add_current_task_to_delayed_list(ticks_to_delay, false);
        }

        already_yielded = task_resume_all();
    } else {
        mtCOVERAGE_TEST_MARKER!();
    }

    if !already_yielded {
        portYIELD_WITHIN_API!();
    } else {
        mtCOVERAGE_TEST_MARKER!();
    }
}
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
    () => {};
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
