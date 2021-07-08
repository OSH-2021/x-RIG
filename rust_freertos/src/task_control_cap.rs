use crate::kernel::*;
use crate::ffi::*;
use crate::list::*;
use crate::port;
use crate::port::*;
use crate::projdefs::FreeRtosError;
use crate::task_global;
use crate::*;
use crate::regs::*;
use std::ops::FnOnce;
use std::mem;
use std::sync::{Arc, RwLock, Weak};
use crate::arch_structures_TCB::*;
use crate::CNode::*;
use crate::types::*;
use crate::task_global::*;
use crate::task_ipc::*;


pub const MAX_CSlots: usize = wordBits as usize;

pub const L2_BITMAP_SIZE: usize = (256 + (1 << 6) - 1) / (1 << 6);
extern "C" {
    static mut ksReadyQueues: [tcb_queue_t; 256];   // TODO -> READY_TASK_LISTS
    static mut ksReadyQueuesL1Bitmap: [u64; 1];
    static mut ksReadyQueuesL2Bitmap: [[u64; L2_BITMAP_SIZE]; 1];
    static mut current_extra_caps: extra_caps_t;
    static mut current_syscall_error: syscall_error_t;
    static mut current_fault: seL4_Fault_t;
    static mut current_lookup_fault: lookup_fault_t;
    pub fn getRestartPC(thread: *mut tcb_t) -> u64;
    pub fn setNextPC(thread: *mut tcb_t, v: u64);
    pub fn lookupIPCBuffer(isReceiver: bool, thread: *mut tcb_t) -> *mut u64;
    pub fn sanitiseRegister(reg_id: usize, v: u64, archInfo: bool) -> u64;
    pub fn Arch_setTCBIPCBuffer(thread: *mut tcb_t, bufferAddr: u64);
    pub fn Arch_postModifyRegisters(tptr: *mut tcb_t);
    pub fn Arch_performTransfer(arch: u64, tcb_src: *mut tcb_t, dest: *mut tcb_t) -> u64;
    pub fn sameObjectAs(cap_a: cap_t, cap_b: cap_t) -> bool_t;
    // fn kprintf(format: *const u8, ...) -> u64;
}
// TODO how to convert??
// indeed we should have a lock on it
// static mut current_tcb_handle : TaskHandle = get_current_task_handle!();
// pub static mut ksCurThread: get_ptr_from_handle!(get_current_task_handle!()): *mut tcb_t = get_tcb_from_handle_mut!(current_tcb_handle);  //  TODO->  CURRENT_TCB


/* Task states returned by eTaskGetState. */
#[derive(Copy, Clone, Debug)]
#[repr(u8)]
pub enum TaskState {
    running = 0,
    ready = 1,
    blocked = 2,
    suspended = 3,
    deleted = 4,
    InActive,                           //  seL4
    Restart,                            //  seL4
    BlockedOnReceive,                   //  seL4
    BlockedOnSend,                      //  seL4
    BlockedOnReply,                     //  seL4
    BlockedOnNotificn,                  //  seL4
    Idle,                               //  seL4
}
pub type _thread_state = TaskState;

pub enum thread_control_flag {
    thread_control_update_priority = 0x1,
    thread_control_update_ipc_buffer = 0x2,
    thread_control_update_space = 0x4,
    thread_control_update_mcp = 0x8,
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

    // #[cfg(feature = "configUSE_CAPS")]
    // arch : ???,  //  暂时先不考虑? arch里面有很多register,单独拿出来了
    task_state : TaskState, // TODO: add ThreadStateType | translated to task (from thread)
    //  TODO
    // bound_notification : Option<Notification> // notifications are not necessarily bound to tcb freertos已经有notification了我们还要写吗？
    // task_fault : FaultType,
    // lookup_failure : LookupFault,
    // domain : Domain,
    max_ctrl_prio : UBaseType, // same as freertos prio
    // time_slice : UBaseType, // freertos应该也有内置的时间片吧 在哪？
    fault_handler : UBaseType, // used only once in qwq
    ipc_buffer : UBaseType, // 总觉得这个和stream buffer很像
    pub registers : [word_t; n_contextRegisters],
    pub ctable: Arc<RwLock<CTable>>,
}

// pub unsafe fn suspend(target: *mut tcb_t) {
//     cancelIPC(target);
//     setThreadState(target, TaskState::InActive);
//     tcbSchedDequeue(target);
// }

// #[no_mangle]
// pub unsafe fn restart(target: *mut tcb_t) {
//     if isBlocked(target) != 0 {
//         cancelIPC(target);
//         setupReplyMaster(target);
//         setThreadState(target, _thread_state::Restart);
//         tcbSchedEnqueue(target);
//         possibleSwitchTo(target);
//     }
// }
// TODO
// pub unsafe fn setThreadState(tptr: *mut tcb_t, ts : TaskState) {
//     let tcbState = &mut (*tptr).task_state;
//     thread_state_ptr_set_tsType(tcbState, ts); // sure to use sel4's function directly?
//     scheduleTCB(tptr);
// }

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

            // TODO
            task_state: TaskState::Idle,
            // lookup_failure: 0,
            // domain: 0,
            fault_handler: 0,
            max_ctrl_prio: configMAX_PRIORITIES!(),
            ipc_buffer: 0,
            registers : [0; n_contextRegisters],
            ctable: Arc::new(RwLock::new((CTable {
                caps: [cte_t {
                    cap: cap_t {
                        words: [0, 0]
                    },
                    cteMDBNode: mdb_node_t {
                        words: [0, 0]
                    }
                }; MAX_CSlots]
            }))),
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

        let f = Box::new(Box::new(func) as Box<dyn FnOnce()>); // Pass task function as a parameter.
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

    #[cfg(feature = "configUSE_MUTEXES")]
    pub fn get_base_priority(&self) -> UBaseType {
        self.base_priority
    }

    #[cfg(feature = "configUSE_MUTEXES")]
    pub fn set_base_priority(&mut self, new_val: UBaseType) {
        self.base_priority = new_val
    }

    //  TODO add other member's setter, getter and else
    pub fn set_fault_handler(&mut self, faulteq: u64) {
        self.fault_handler = faulteq;
    }

    pub fn set_MCPriority(&mut self, mcp: prio_t) {
        self.max_ctrl_prio = mcp;
    }

    pub fn set_ipc_buffer(&mut self, bufferAddr: u64) {
        self.ipc_buffer = bufferAddr;
    }

    pub fn set_state(&mut self, state: TaskState) {
        self.task_state = state;
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
        let func_to_run = Box::from_raw(func_to_run as *mut Box<dyn FnOnce() + 'static>);
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
pub struct TaskHandle(pub Arc<RwLock<TCB>>);

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
    /// TODO append task to ready list
    pub fn append_task_to_ready_list(&self) -> Result<(), FreeRtosError> {
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
            &task_global::READY_TASK_LISTS[priority as usize],
            Arc::clone(&unwrapped_tcb.state_list_item),
        );
        tracePOST_MOVED_TASK_TO_READY_STATE!(&unwrapped_tcb);
        Ok(())
    }

    /// # Description:
    ///     insert task to the head of the ready list
    /// * Implemented by:
    ///     Lslightly
    /// # Arguments:
    ///   self  the task self
    /// # Return:
    ///   Ok(())
    ///   Err(FreeRtosError)
    pub fn insert_task_to_ready_list(&self) -> Result<(), FreeRtosError> {
        let unwrapped_tcb = get_tcb_from_handle!(self);
        let priority = self.get_priority();

        traceMOVED_TASK_TO_READY_STATE!(&unwrapped_tcb);
        record_ready_priority!(priority);

        //  ???really insert to the head of list???
        list::list_insert(
            &task_global::READY_TASK_LISTS[priority as usize],
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
            /* CURRENT_TCB won't be None. See task_global.rs. */    //  ???
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
            self.append_task_to_ready_list()?;
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

    /// # Description:
    ///     append task to the end of endpoint list
    /// * Implemented by:
    ///     Lslightly
    /// # Arguments:
    ///     self
    /// # Return:
    ///     Ok(())
    ///     Err(FreeRtosError)
    /// TODO return what?
    pub fn append_task_to_endpoint_list(&self, index_queue: u64) -> Result<(), FreeRtosError> {
        let unwrapped_tcb = get_tcb_from_handle!(self);
        list::list_insert_end(&task_global::ENDPOINT_LIST[index_queue as usize], Arc::clone(&unwrapped_tcb.state_list_item));
        Ok(())
    }

    /// # Description:
    ///    delete task from endpoint list
    /// * Implemented by:
    ///    Lslightly
    /// # Arguments:
    ///
    /// # Return:
    ///
    pub fn delete_task_from_endpoint_list(&self, index_queue: u64) -> Result<(), FreeRtosError> {
        let unwrapped_tcb = get_tcb_from_handle!(self);
        list::list_remove(Arc::clone(&unwrapped_tcb.state_list_item));
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

    pub fn set_fault_handler(&self, faulteq: u64) {
        get_tcb_from_handle_mut!(self).set_fault_handler(faulteq);
    }

    pub fn set_MCPriority(&self, mcp: prio_t) {
        get_tcb_from_handle_mut!(self).set_MCPriority(mcp);
    }

    pub fn set_ipc_buffer(&self, bufferAddr: u64) {
        get_tcb_from_handle_mut!(self).set_ipc_buffer(bufferAddr);
    }

    pub fn set_state(&self, state: TaskState) {
        get_tcb_from_handle_mut!(self).set_state(state);
    }

    /// # Description:
    ///    set TaskHandle's slot, fault handler, mcp, priority, CTable, VTable, IPC buffer
    /// * Implemented by:
    ///    Lslightly
    /// # Arguments:
    ///
    /// # Return:
    ///
    pub unsafe fn invokeTCB_ThreadControl(
        &self,
        slot: Arc<cte_t>,
        faultep: u64,
        mcp: prio_t,
        priority: prio_t,
        cRoot_newCap: cap_t,
        cRoot_srcSlot: Arc<cte_t>,
        vRoot_newCap: cap_t,
        vRoot_srcSlot: Arc<cte_t>,
        bufferAddr: u64,
        bufferCap: cap_t,
        bufferSrcSlot: Arc<cte_t>,
        updateFlags: u64,
    ) -> u64 {
        let target_ptr = get_ptr_from_handle!(self);
        // let tCap = cap_thread_cap_new(target_ptr as u64);  //  originally
        let tCap = cap_thread_cap_new(target_ptr as u64);  //  use *mut tcb_t and modify the function, in the function, use "as u64"
        //  Fault Handler
        if updateFlags & thread_control_flag::thread_control_update_space as u64 != 0u64 {
            get_tcb_from_handle_mut!(self).set_fault_handler(faultep);
        }
        //  Max Control Priority
        if updateFlags & thread_control_flag::thread_control_update_mcp as u64 != 0u64 {
            get_tcb_from_handle_mut!(self).set_MCPriority(mcp);
        }
        //  Priority
        if updateFlags & thread_control_flag::thread_control_update_priority as u64 != 0u64 {
            get_tcb_from_handle_mut!(self).set_priority(priority);
        }
        //  CTable
        if updateFlags & thread_control_flag::thread_control_update_space as u64 != 0u64 {
            let rootSlot = Arc::from_raw(tcb_ptr_cte_ptr(target_ptr, tcb_cnode_index::tcbCTable as u64));
            let e = cteDelete(Arc::new(RwLock::new(*rootSlot.clone())), 1u64);
            if e != 0u64 {
                return e;
            }
            if sameObjectAs(cRoot_newCap, (*cRoot_srcSlot).cap) != 0u64
                && sameObjectAs(tCap, (*slot).cap) != 0u64
            {
                cteInsert(cRoot_newCap, Arc::new(RwLock::new(*cRoot_srcSlot.clone())), Arc::new(RwLock::new(*rootSlot.clone())));
            }
        }
        //  VTable
        if updateFlags & thread_control_flag::thread_control_update_space as u64 != 0u64 {
            let rootSlot = Arc::from_raw(tcb_ptr_cte_ptr(target_ptr, tcb_cnode_index::tcbVTable as u64));
            let e = cteDelete(Arc::new(RwLock::new(*rootSlot.clone())), 1u64);
            if e != 0u64 {
                return e;
            }
            if sameObjectAs(vRoot_newCap, (*vRoot_srcSlot).cap) != 0u64
                && sameObjectAs(tCap, (*slot).cap) != 0u64
            {
                cteInsert(vRoot_newCap, Arc::new(RwLock::new(*vRoot_srcSlot.clone())), Arc::new(RwLock::new(*rootSlot.clone())));
            }
        }
        //  IPC Buffer
        if updateFlags & thread_control_flag::thread_control_update_ipc_buffer as u64 != 0u64 {
            let bufferSlot = Arc::from_raw(tcb_ptr_cte_ptr(target_ptr, tcb_cnode_index::tcbBuffer as u64));
            let e = cteDelete(Arc::new(RwLock::new(*bufferSlot.clone())), 1u64);
            if e != 0u64 {
                return e;
            }
            get_tcb_from_handle_mut!(self).set_ipc_buffer(bufferAddr);
            // Arch_setTCBIPCBuffer(target_ptr, bufferAddr);    //  not appear in source code?  TODO
            if Arc::as_ptr(&bufferSrcSlot) as u64 != 0u64
                && sameObjectAs(bufferCap, (*bufferSrcSlot).cap) != 0u64
                && sameObjectAs(tCap, (*slot).cap) != 0u64
            {
                cteInsert(bufferCap, Arc::new(RwLock::new(*bufferSrcSlot.clone())), Arc::new(RwLock::new(*bufferSlot.clone())));
            }
            if self == &get_current_task_handle!() {
                // rescheduleRequired();
            }
        }
        0u64
    }
    // original name invokeTCB_CopyRegisters (add From to clarify)
    pub unsafe fn invokeTCB_CopyRegistersFrom(
        dest: &mut Self,
        src: &mut Self,
        suspendSource: bool,
        resumeTarget: bool,
        transferFrame: bool,
        transferInteger: bool,
        transferArch: u64,
    ) -> u64 {
        let dest_ptr = get_ptr_from_handle!(dest);
        let src_ptr = get_ptr_from_handle!(src);
        if suspendSource != false {
            //  suspend(dest_ptr);  //  originally
            suspend_task(dest);
        }
        if resumeTarget != false {
            // restart(dest_ptr);
            resume_task(dest)
        }
        if transferFrame != false {
            let mut i: usize = 0;
            while i < n_frameRegisters {
                let v = getRegister(src_ptr, frameRegisters[i]);
                setRegister(dest_ptr, frameRegisters[i], v);
                i += 1;
            }
            let pc = getRestartPC(dest_ptr);
            setNextPC(dest_ptr, pc);
        }
        if transferInteger != false {
            let mut i: usize = 0;
            while i < n_gpRegisters {
                let v = getRegister(src_ptr, gpRegisters[i]);
                setRegister(dest_ptr, gpRegisters[i], v);
                i += 1;
            }
        }
        Arch_postModifyRegisters(dest_ptr);
        let current_cur = get_current_task_handle_mut!();
        let current_ptr = get_ptr_from_handle!(current_cur);
        if dest_ptr == node_state!(current_ptr) {
            // rescheduleRequired();
            // TODO reschedule
        }
        Arch_performTransfer(transferArch, src_ptr, dest_ptr)
    }

    pub unsafe fn invokeTCB_ReadRegisters(
        src: &mut Self,
        suspendSource: bool,
        n: u64,
        arch: u64,
        call: bool,
    ) -> u64 {
        let src_ptr = get_ptr_from_handle!(src);
        let current_cur = get_current_task_handle_mut!();
        let current_ptr = get_ptr_from_handle!(current_cur);
        let thread = node_state!(current_cur);
        let thread_ptr = get_ptr_from_handle!(thread);
        if suspendSource != false { // 0u64 as false
            // suspend(src_tcb);
            suspend_task(src);
        }
        let current_cur = get_current_task_handle_mut!();
        let current_ptr = get_ptr_from_handle!(current_cur);
        let e = Arch_performTransfer(arch, src_ptr, node_state!(current_ptr));
        if e != 0u64 {
            return e;
        }
        if call != false {
            let ipcBuffer = lookupIPCBuffer(true, thread_ptr);
            let mut i: usize = 0;
            while i < n as usize && i < n_frameRegisters && i < n_msgRegisters {
                setRegister(
                    thread_ptr,
                    msgRegisters[i],
                    getRegister(src_ptr, frameRegisters[i]),
                );
                i += 1;
            }
            if ipcBuffer as u64 != 0u64 && i < n as usize && i < n_frameRegisters {
                while i < n as usize && i < n_frameRegisters {
                    *ipcBuffer.offset((i + 1) as isize) = getRegister(src_ptr, frameRegisters[i]);
                    i += 1;
                }
            }
            let j = i;
            i = 0;
            while i < n_gpRegisters
                && i + n_frameRegisters < n as usize
                && i + n_frameRegisters < n_msgRegisters
            {
                setRegister(
                    thread_ptr,
                    msgRegisters[i + n_frameRegisters],
                    getRegister(src_ptr, gpRegisters[i]),
                );
                i += 1;
            }
            if ipcBuffer as u64 != 0u64 && i < n_gpRegisters && i + n_frameRegisters < n as usize {
                while i < n_gpRegisters && i + n_frameRegisters < n as usize {
                    *ipcBuffer.offset((i + n_frameRegisters + 1) as isize) =
                        getRegister(src_ptr, gpRegisters[i]);
                    i += 1;
                }
            }
            setRegister(
                thread_ptr,
                msgInfoRegister,
                wordFromMessageInfo(seL4_MessageInfo_new(0, 0, 0, (i + j) as u64)),
            );
        }
        // setThreadState(thread, TaskState::running);
        (*thread).set_state(TaskState::running);
        0u64
    }

    pub unsafe fn invokeTCB_WriteRegisters(
        dest: &mut Self,
        resumeTarget: bool,
        mut n: u64,
        arch: u64,
        buffer: *mut u64
    ) -> u64 {
        let dest_ptr = get_ptr_from_handle!(dest);

        let current_cur = get_current_task_handle_mut!();
        let current_ptr = get_ptr_from_handle!(current_cur);
        let e = Arch_performTransfer(arch, node_state!(current_ptr), dest_ptr);
        if e != 0u64 {
            return e;
        }
        if n as usize > n_frameRegisters + n_gpRegisters {
            n = (n_frameRegisters + n_gpRegisters) as u64;
        }
        let archInfo: bool = false; //  Arch_getSanitiseRegisterInfo(dest); //  originally
        let mut i: usize = 0;
        while i < n_frameRegisters && i < n as usize {
            setRegister(
                dest_ptr,
                frameRegisters[i],
                sanitiseRegister(
                    frameRegisters[i],
                    getSyscallArg((i + 2) as u64, buffer),
                    archInfo,
                ),
            );
            i += 1;
        }
        i = 0;
        while i < n_gpRegisters && i + n_frameRegisters < n as usize {
            setRegister(
                dest_ptr,
                gpRegisters[i],
                sanitiseRegister(
                    gpRegisters[i],
                    getSyscallArg((i + n_frameRegisters + 2) as u64, buffer),
                    archInfo,
                ),
            );
            i += 1;
        }
        let pc = getRestartPC(dest_ptr);
        setNextPC(dest_ptr, pc);
        Arch_postModifyRegisters(dest_ptr);
        if resumeTarget != false {
            // restart(dest_ptr);
            resume_task(dest);
        }
        // if dest_ptr == node_state!(get_ptr_from_handle!(get_current_task_handle!())) {
        if *dest == get_current_task_handle!() {
            // rescheduleRequired();
            // TODO reschedule
        }
        0u64
    }

    //  ???notification TODO
    pub unsafe fn invokeTCB_NotificationControl(
        &mut self,// tcb: *mut tcb_t,
        ntfnPtr: *mut notification_t
    ) -> u64 {
        if ntfnPtr as u64 != 0u64 {
            // bindNotification(tcb, ntfnPtr);
        } else {
            // unbindNotification(tcb);
        }
        0u64
    }
}
    pub unsafe fn setMRs_lookup_failure(
        receiver: *mut tcb_t,
        receiveIPCBuffer: *mut u64,
        luf: lookup_fault_t,
        offset: u32,
    ) -> u32 {
        let lufType = lookup_fault_get_lufType(luf);
        let i = setMR(receiver, receiveIPCBuffer, offset, lufType + 1);
        if lufType == lookup_fault_tag_t::lookup_fault_invalid_root as u64 {
            return i;
        } else if lufType == lookup_fault_tag_t::lookup_fault_missing_capability as u64 {
            return setMR(
                receiver,
                receiveIPCBuffer,
                offset + 1,
                lookup_fault_missing_capability_get_bitsLeft(luf),
            );
        } else if lufType == lookup_fault_tag_t::lookup_fault_depth_mismatch as u64 {
            setMR(
                receiver,
                receiveIPCBuffer,
                offset + 1,
                lookup_fault_depth_mismatch_get_bitsLeft(luf),
            );
            return setMR(
                receiver,
                receiveIPCBuffer,
                offset + 2,
                lookup_fault_depth_mismatch_get_bitsFound(luf),
            );
        } else if lufType == lookup_fault_tag_t::lookup_fault_guard_mismatch as u64 {
            setMR(
                receiver,
                receiveIPCBuffer,
                offset + 1,
                lookup_fault_guard_mismatch_get_bitsLeft(luf),
            );
            setMR(
                receiver,
                receiveIPCBuffer,
                offset + 2,
                lookup_fault_guard_mismatch_get_bitsFound(luf),
            );
            return setMR(
                receiver,
                receiveIPCBuffer,
                offset + 3,
                lookup_fault_guard_mismatch_get_bitsFound(luf),
            );
        }
        panic!("Invalid lookup failure");
    }


    pub unsafe fn setMRs_syscall_error(
        thread: *mut tcb_t,
        receiveIPCBuffer: *mut u64,
    ) -> u64 {
        if current_syscall_error.type_ == seL4_Error::seL4_InvalidArgument as u64 {
            return setMR(
                thread,
                receiveIPCBuffer,
                0,
                current_syscall_error.invalidArgumentNumber,
            ) as u64;
        } else if current_syscall_error.type_ == seL4_Error::seL4_InvalidCapability as u64 {
            return setMR(
                thread,
                receiveIPCBuffer,
                0,
                current_syscall_error.invalidCapNumber,
            ) as u64;
        } else if current_syscall_error.type_ == seL4_Error::seL4_IllegalOperation as u64 {
            return 0u64;
        } else if current_syscall_error.type_ == seL4_Error::seL4_RangeError as u64 {
            setMR(
                thread,
                receiveIPCBuffer,
                0,
                current_syscall_error.rangeErrorMin,
            );
            return setMR(
                thread,
                receiveIPCBuffer,
                1,
                current_syscall_error.rangeErrorMax,
            ) as u64;
        } else if current_syscall_error.type_ == seL4_Error::seL4_AlignmentError as u64 {
            return 0u64;
        } else if current_syscall_error.type_ == seL4_Error::seL4_FailedLookup as u64 {
            setMR(
                thread,
                receiveIPCBuffer,
                0,
                (current_syscall_error.failedLookupWasSource != 0u64) as u64,
            );
            return setMRs_lookup_failure(thread, receiveIPCBuffer, current_lookup_fault, 1) as u64;
        } else if current_syscall_error.type_ == seL4_Error::seL4_TruncatedMessage as u64
            || current_syscall_error.type_ == seL4_Error::seL4_DeleteFirst as u64
            || current_syscall_error.type_ == seL4_Error::seL4_RevokeFirst as u64
        {
            return 0u64;
        } else if current_syscall_error.type_ == seL4_Error::seL4_NotEnoughMemory as u64 {
            return setMR(
                thread,
                receiveIPCBuffer,
                0,
                current_syscall_error.memoryLeft,
            ) as u64;
        }
        panic!("Invalid syscall error");
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

#[macro_export]
macro_rules! get_ptr_from_handle {  //  the raw pointer is unsafe
    ($handle: expr) => {
        match $handle.0.write() {
            Ok(mut a) => &mut *a as *mut tcb_t,
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
        #[cfg(feature = "INCLUDE_xTaskAbortDelay")]
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
        portRESET_READY_PRIORITY!(unwrapped_cur.get_priority(), get_top_ready_priority!());     //  ??? this macro doesn't do anything
    } else {
        trace!("Returned not 0");
        mtCOVERAGE_TEST_MARKER!();  //  ??? any use? mark?
    }

    trace!("Remove succeeded");
    {
        #![cfg(feature = "INCLUDE_vTaskSuspend")]
        if ticks_to_wait == portMAX_DELAY && can_block_indefinitely {
            /* Add the task to the suspended task list instead of a delayed task
            list to ensure it is not woken by a timing event.  It will block
            indefinitely. */
            let cur_state_list_item = unwrapped_cur.get_state_list_item();
            list::list_insert_end(&task_global::SUSPENDED_TASK_LIST, cur_state_list_item);
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
                list::list_insert(&task_global::OVERFLOW_DELAYED_TASK_LIST, cur_state_list_item);
            } else {
                /* The wake time has not overflowed, so the current block list
                is used. */
                list::list_insert(&task_global::DELAYED_TASK_LIST, unwrapped_cur.get_state_list_item());

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
    if list_is_empty(&task_global::DELAYED_TASK_LIST) {
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
        let mut temp = get_owner_of_head_entry(&task_global::DELAYED_TASK_LIST);
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
///  `task_to_delete` The handle of the task to be deleted.  Passing None will
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
        set_task_number!(get_task_number!() + 1);   //  ???

        if pxtcb == get_current_task_handle!() {    //  ???
            /* A task is deleting itself.  This cannot complete within the
            task itself, as a context switch to another task is required.
            Place the task in the termination list.  The idle task will
            check the termination list and free up any memory allocated by
            the scheduler for the TCB and stack of the deleted task. */
            list::list_insert_end(&task_global::TASKS_WAITING_TERMINATION, pxtcb.get_state_list_item());

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
            reset_next_task_unblock_time(); //  ???
        }
        // FIXME
        //traceTASK_DELETE!(task_to_delete);
    }
    taskEXIT_CRITICAL!();

    /* Force a reschedule if it is the currently running task that has just
    been deleted. */
    if get_scheduler_suspended!() > 0 { //  ???
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
pub fn suspend_task(task_to_suspend: &mut TaskHandle) { //  is &mut added OK? TODO to be explained
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
        list_insert_end(&task_global::SUSPENDED_TASK_LIST, unwrapped_tcb.get_state_list_item());
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

    if task_to_suspend == get_current_task_handle_mut!() {  //  Does &mut equal really mean task to suspend is current task?
        if get_scheduler_running!() {
            /* The current task has just been suspended. */
            assert!(get_scheduler_suspended!() == 0);
            portYIELD_WITHIN_API!();
        } else {
            /* The scheduler is not running, but the task that was pointed
            to by pxCurrentTCB has just been suspended and pxCurrentTCB
            must be adjusted to point to a different task. */
            if current_list_length(&task_global::SUSPENDED_TASK_LIST) != get_current_number_of_tasks!() {
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
    if is_contained_within(&task_global::SUSPENDED_TASK_LIST, &tcb.get_state_list_item()) {
        /* Has the task already been resumed from within an ISR? */
        if !is_contained_within(&task_global::PENDING_READY_LIST, &tcb.get_event_list_item()) {
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
pub fn resume_task(task_to_resume: &mut TaskHandle) {   //  does &mut OK? TODO to be explained
    trace!("resume task called!");
    let mut unwrapped_tcb = get_tcb_from_handle!(task_to_resume);

    if task_to_resume != get_current_task_handle_mut!() {   //  TODO &mut OK? to be explained
        taskENTER_CRITICAL!();
        {
            if task_is_tasksuspended(&task_to_resume) {
                traceTASK_RESUME!(&unwrapped_tcb);

                /* As we are in a critical section we can access the ready
                lists even if the scheduler is suspended. */
                list_remove(unwrapped_tcb.get_state_list_item());
                task_to_resume.append_task_to_ready_list();

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

#[inline]
pub unsafe fn setMR(
    receiver: *mut tcb_t,
    receiveIPCBuffer: *mut u64,
    offset: u32,
    reg: u64,
) -> u32 {
    if offset >= n_msgRegisters as u32 {
        if receiveIPCBuffer as u64 != 0u64 {
            *receiveIPCBuffer.offset((offset + 1) as isize) = reg;
            return offset + 1;
        } else {
            return n_msgRegisters as u32;
        }
    } else {
        setRegister(receiver, msgRegisters[offset as usize], reg);
        return offset + 1;
    }
}
