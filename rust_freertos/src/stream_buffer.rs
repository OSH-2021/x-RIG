use lazy_static::__Deref;

use crate::config::*;
use crate::kernel::*;
use crate::list::list_remove;
use crate::port::*;
#[cfg(feature = "configUSE_CAPS")]
use crate::task_control_cap::*;
#[cfg(not(feature = "configUSE_CAPS"))]
use crate::task_control::*;
use crate::trace::*;
use crate::*;
use std::fmt;
use std::ops::DerefMut;
use std::sync::{Arc, RwLock, RwLockReadGuard, RwLockWriteGuard, Weak};

#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub enum StreamBufferError {
    StreamBufferTriggerLevelOverflow,
    StreamBufferFull,
    StreamBufferEmpty,
}
impl fmt::Display for StreamBufferError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            StreamBufferError::StreamBufferTriggerLevelOverflow => {
                write!(f, "TriggerLevelOverflow")
            }
            StreamBufferError::StreamBufferFull => write!(f, "StreamBufferFull"),
            StreamBufferError::StreamBufferEmpty => write!(f, "StreamBufferEmpty"),
        }
    }
}

pub const sbFLAGS_IS_MESSAGE_BUFFER: u8 = 1;
pub const sbFLAGS_IS_STATICALLY_ALLOCATED: u8 = 2;
/// *Descrpition:
/// Definition of  Stream Buffer
///
/// The source code : stream_buffer.c  
///
/// *Implemented by Chen Wenjie
#[derive(Clone)]
pub struct StreamBufferDef {
    xTail: UBaseType,
    xHead: UBaseType,
    xLength: UBaseType,
    xTriggerLevelBytes: UBaseType,
    xTaskWaitingToReceive: Option<TaskHandle>,
    xTaskWaitingToSend: Option<TaskHandle>,
    pucBuffer: u8,
    ucFlag: u8,

    uxStreamBufferNumber: UBaseType,
}

impl StreamBufferDef {
    pub fn new() -> Self {
        StreamBufferDef {
            xTail: 0,
            xHead: 0,
            xLength: 0,
            xTriggerLevelBytes: 1,
            xTaskWaitingToReceive: None,
            xTaskWaitingToSend: None,
            pucBuffer: 0,
            ucFlag: 0,
            uxStreamBufferNumber: 0,
        }
    }

    /// get the number of bytes in buffer
    /// * Implemented by Chen Wenjie
    /// * Arguments
    ///     self             : The handle of the stream buffer to be read.
    /// * Source code : stream_buffer.c 1229-1247
    ///
    /// * Return
    ///     the number
    pub fn BytesInBuffer(&self) -> UBaseType {
        let mut Count: UBaseType;

        Count = self.xLength + self.xHead - self.xTail;

        if Count >= self.xLength {
            Count -= self.xLength;
        } else {
            mtCOVERAGE_TEST_MARKER!();
        }

        Count
    }


    /// Initialise the buffer
    /// * Implemented by Chen Wenjie
    /// * Arguments
    ///     self                : The handle of the stream buffer to be initialised.
    ///     pucBuffer           : The initial data in buffer
    ///     BufferSizeBytes    : The size, in bytes, of the buffer pointed to by the pucStreamBufferStorageArea parameter.
    ///     TriggerLevelBytes  : The number of bytes that must be in the stream buffer before a task that is blocked on the stream buffer to wait for data is
    ///                           moved out of the blocked state.  For example, if a task is blocked on a read of an empty stream buffer that has a trigger level of 1 then the task will be
    ///                           unblocked when a single byte is written to the buffer or the task's block time expires.  As another example, if a task is blocked on a read of an empty
    ///                           stream buffer that has a trigger level of 10 then the task will not be unblocked until the stream buffer contains at least 10 bytes or the task's
    ///                           block time expires.  If a reading task's block time expires before the trigger level is reached then the task will still receive however many bytes
    ///                           are actually available.  Setting a trigger level of 0 will result in a trigger level of 1 being used.  It is not valid to specify a trigger level
    ///                           that is greater than the buffer size.     
    /// * Source code : stream_buffer.c 1250-1274
    ///
    /// * Return
    ///     
    pub fn InitialiseNewStreamBuffer(
        &mut self,
        pucBuffer: u8,
        BufferSizeBytes: UBaseType,
        TriggerLevelBytes: UBaseType,
        ucFlag: u8,
    ) {
        self.pucBuffer = pucBuffer;
        self.ucFlag = ucFlag;
        self.xTriggerLevelBytes = TriggerLevelBytes;
        self.xLength = BufferSizeBytes;
    }
}
#[derive(Clone)]
pub struct StreamBufferHandle(Arc<RwLock<StreamBufferDef>>);

impl From<Weak<RwLock<StreamBufferDef>>> for StreamBufferHandle {
    fn from(weak_link: Weak<RwLock<StreamBufferDef>>) -> Self {
        StreamBufferHandle(
            weak_link
                .upgrade()
                .unwrap_or_else(|| panic!("Owner is not set")),
        )
    }
}

impl From<StreamBufferHandle> for Weak<RwLock<StreamBufferDef>> {
    fn from(stream_buffer: StreamBufferHandle) -> Self {
        Arc::downgrade(&stream_buffer.0)
    }
}

/// Construct a StreamBufferHandle with a StreamBuffer. */
/// * Implemented by: Chen Wenjie.
/// * C implementation:
///
/// # Arguments
/// * `stream_buffer`: The StreamBuffer that we want to get StreamBufferHandle from.
///
/// # Return
///
/// The created StreamBufferHandle.
pub fn from(stream_buffer: StreamBufferDef) -> StreamBufferHandle {
    // TODO: Implement From.
    StreamBufferHandle(Arc::new(RwLock::new(stream_buffer)))
}

impl StreamBufferHandle {
    pub fn from_arc(arc: Arc<RwLock<StreamBufferDef>>) -> Self {
        StreamBufferHandle(arc)
    }

    pub fn get_buffer(&self) -> u8 {
        get_streambuffer_from_handle!(self).pucBuffer
    }

    pub fn get_length(&self) -> UBaseType {
        get_streambuffer_from_handle!(self).xLength
    }

    pub fn get_triggerlevelbytes(&self) -> UBaseType {
        get_streambuffer_from_handle!(self).xTriggerLevelBytes
    }

    pub fn get_flag(&self) -> u8 {
        get_streambuffer_from_handle!(self).ucFlag
    }

    pub fn get_streambuffernumber(&self) -> UBaseType {
        get_streambuffer_from_handle!(self).uxStreamBufferNumber
    }

    // pub fn get_tasktoreceive_notifystate(&self)->Option<TaskHandle>{
    //     get_streambuffer_from_handle!(self).xTaskWaitingToReceive
    // }

    // pub fn get_tasktosend(&self)-> TaskHandle{
    //     get_streambuffer_from_handle!(self).xTaskWaitingToSend.unwrap()
    // }

    /// Reset a stream buffer
    /// * Implemented by Chen Wenjie
    /// * Arguments
    ///     self             : The handle of the stream buffer to be reset.
    ///
    /// * The source code : stream_buffer.c  line416 - 462
    /// * Return
    ///
    pub fn StreamBufferReset(&mut self) -> Result<(), StreamBufferError> {
        let mut unwrap_streambuffer = get_streambuffer_from_handle_mut!(self);

        let Buffer = self.get_buffer();
        let Length = self.get_length();
        let TriggerLevelBytes = self.get_triggerlevelbytes();
        let Flag = self.get_flag();
        let mut uxStreamBufferNumber = self.get_streambuffernumber();

        taskENTER_CRITICAL!();

        if (!unwrap_streambuffer.xTaskWaitingToReceive.is_none())
            && (!unwrap_streambuffer.xTaskWaitingToSend.is_none())
        {
            unwrap_streambuffer.InitialiseNewStreamBuffer(Buffer, Length, TriggerLevelBytes, Flag);

            unwrap_streambuffer.uxStreamBufferNumber = uxStreamBufferNumber;
            traceSTREAM_BUFFER_RESET!();
        }
        taskEXIT_CRITICAL!();
        Ok(())
    }

    /// Set the trigger level of a stream buffer
    /// * Implemented by Chen Wenjie
    /// * Arguments
    ///     self             : The handle of the stream buffer being updated.
    ///     xTriggerLevel    : The new trigger level for the stream buffer.
    /// * The source code    : stream_buffer.c  line465 - 492    
    /// * Return
    ///     
    pub fn StreamBufferSetTriggerLevel(
        &mut self,
        xTriggerLevel: UBaseType,
    ) -> Result<(), StreamBufferError> {
        let mut unwrap_streambuffer = get_streambuffer_from_handle_mut!(self);

        if xTriggerLevel == 0 {
            xTriggerLevel == 1;
        }

        if xTriggerLevel <= unwrap_streambuffer.xLength {
            unwrap_streambuffer.xTriggerLevelBytes = xTriggerLevel;
            return Ok(());
        }

        Err(StreamBufferError::StreamBufferTriggerLevelOverflow)
    }

    /// Queries a stream buffer to see how much free space it contains, which is equal to the amount of data that
    /// can be sent to the stream buffer before it is full.
    ///
    /// * Implemented by Chen Wenjie
    /// * Arguments
    ///     self             : The handle of the stream buffer being queried.
    ///     xTriggerLevel    : The new trigger level for the stream buffer.
    /// * Return
    ///     The number of bytes that can be read from the stream buffer before the stream buffer would be full.
    pub fn StreamBufferBytesAvailable(&mut self) -> UBaseType {
        get_streambuffer_from_handle!(self).BytesInBuffer()
    }

    pub fn StreamBufferSpacesAvailable(&self) -> UBaseType {
        let mut unwrap_streambuffer = get_streambuffer_from_handle!(self);

        let mut xSpace: UBaseType;

        xSpace = unwrap_streambuffer.xLength + unwrap_streambuffer.xLength;
        xSpace -= (unwrap_streambuffer.xHead + 1 as UBaseType);

        if xSpace >= unwrap_streambuffer.xLength {
            xSpace -= unwrap_streambuffer.xLength;
        } else {
            mtCOVERAGE_TEST_MARKER!();
        }
        xSpace
    }

    /// Create a StreamBuffer in static allocation
    /// * Implemented by Chen Wenjie
    /// * Arguments
    ///     xBufferSizeBytes    : The size, in bytes, of the buffer pointed to by the pucStreamBufferStorageArea parameter.
    ///     xTriggerLevelBytes  : The number of bytes that must be in the stream buffer before a task that is blocked on the stream buffer to wait for data is
    ///                           moved out of the blocked state.  For example, if a task is blocked on a read of an empty stream buffer that has a trigger level of 1 then the task will be
    ///                           unblocked when a single byte is written to the buffer or the task's block time expires.  As another example, if a task is blocked on a read of an empty
    ///                           stream buffer that has a trigger level of 10 then the task will not be unblocked until the stream buffer contains at least 10 bytes or the task's
    ///                           block time expires.  If a reading task's block time expires before the trigger level is reached then the task will still receive however many bytes
    ///                           are actually available.  Setting a trigger level of 0 will result in a trigger level of 1 being used.  It is not valid to specify a trigger level
    ///                           that is greater than the buffer size.    
    /// * Return
    ///     The created stream buffer handle
    pub fn StreamBufferGenericCreate(
        xBufferSizeBytes: UBaseType,
        xTriggerLevelBytes: UBaseType,
        xIsMessageBuffer: bool,
        pucStreamBufferStorageArea: u8,
    ) -> Self {
        let mut ucFlag: u8;
        let mut streambuffer = StreamBufferDef::new();
        if xIsMessageBuffer == true {
            ucFlag = sbFLAGS_IS_MESSAGE_BUFFER;
        } else {
            ucFlag = 0;
        }

        if xTriggerLevelBytes == 0 {
            xTriggerLevelBytes == 1;
        }

        streambuffer.InitialiseNewStreamBuffer(
            pucStreamBufferStorageArea,
            xBufferSizeBytes,
            xTriggerLevelBytes,
            ucFlag,
        );

        from(streambuffer)
    }

    /// Write to a stream buffer from a task.
    ///
    /// * Implemented by Chen Wenjie
    /// * Arguments
    ///     self             : The handle of the stream buffer to which a stream is being sent.
    ///     TxData           : A pointer to the buffer that holds the bytes to be copied into the stream buffer.
    ///     DataLengthBytes  : The maximum number of bytes to copy from pvTxData into the stream buffer.
    ///     TicksToWait      : The maximum amount of time the task should remain in the Blocked state to wait for enough space to become available in the stream
    ///         buffer, should the stream buffer contain too little space to hold the another xDataLengthBytes bytes.  The block time is specified in tick periods,            
    ///         so the absolute time it represents is dependent on the tick frequency.  The macro pdMS_TO_TICKS() can be used to convert a time specified in milliseconds
    ///         into a time specified in ticks.  Setting xTicksToWait to portMAX_DELAY will cause the task to wait indefinitely (without timing out), provided
    ///         INCLUDE_vTaskSuspend is set to 1 in FreeRTOSConfig.h.  If a task times out before it can write all xDataLengthBytes into the buffer it will still write
    ///         as many bytes as possible.  A task does not use any CPU time when it is in the blocked state.
    ///
    /// * The Resource Code : stream_buffer.c 539-668
    /// * Return
    ///     The number of bytes written to the stream buffer.  If a task times out before it can write all xDataLengthBytes into the buffer it will still
    ///     write as many bytes as possible.
    pub fn StreamBufferSend(
        &mut self,
        TxData: u8,
        DataLengthBytes: UBaseType,
        mut TicksToWait: TickType,
    ) -> UBaseType {
        let mut unwrap_streambuffer = get_streambuffer_from_handle_mut!(self);

        let mut Return: UBaseType = 0;
        let mut Space: UBaseType = 0;
        let mut RequiredSpace: UBaseType = DataLengthBytes;
        let mut TimeOut: time_out = Default::default();
        let mut MaxReportedSpace: UBaseType = 0;

        MaxReportedSpace = unwrap_streambuffer.xLength - 1;

        if (unwrap_streambuffer.ucFlag & sbFLAGS_IS_MESSAGE_BUFFER) != 0 {
            RequiredSpace += sbBYTES_TO_STORE_MESSAGE_LENGTH!();

            assert!(RequiredSpace > DataLengthBytes);

            if RequiredSpace > MaxReportedSpace {
                TicksToWait = 0;
            } else {
                mtCOVERAGE_TEST_MARKER!();
            }
        } else {
            if (RequiredSpace > MaxReportedSpace) {
                RequiredSpace = MaxReportedSpace;
            } else {
                mtCOVERAGE_TEST_MARKER!();
            }
        }

        if TicksToWait != 0 {
            task_set_time_out_state(&mut TimeOut);

            while (task_check_for_timeout(&mut TimeOut, &mut TicksToWait) == false) {
                taskENTER_CRITICAL!();

                Space = unwrap_streambuffer.BytesInBuffer();

                if Space < RequiredSpace {
                    //TaskNotifyStateClear();

                    //assert!(unwrap_streambuffer.xTaskWaitingToSend == NULL);
                    unwrap_streambuffer.xTaskWaitingToSend = Some(get_current_task_handle!());
                } else {
                    taskEXIT_CRITICAL!();
                    break;
                }

                taskEXIT_CRITICAL!();
            }
        } else {
            mtCOVERAGE_TEST_MARKER!();
        }

        // Return = self.WriteMessageToBuffer(TxData, DataLengthBytes, Space, RequiredSpace);

        unwrap_streambuffer.pucBuffer = TxData;
        unwrap_streambuffer.xTail +=1;
        unwrap_streambuffer.xLength += 1;



        
        traceSTREAM_BUFFER_SEND!();

        if (unwrap_streambuffer.BytesInBuffer() >= unwrap_streambuffer.xTriggerLevelBytes) {
            send_completed(unwrap_streambuffer.deref_mut());
        } else {
            mtCOVERAGE_TEST_MARKER!();
        }


        1   
        
    }

    /// Receive to a stream buffer from a task.
    ///
    /// * Implemented by Chen Wenjie
    /// * Arguments
    ///     self             : The handle of the stream buffer to which a stream is being sent.
    ///     RxData           : A pointer to the buffer that holds the bytes to be read from the stream buffer.
    ///     DataLengthBytes  : The maximum number of bytes to read from pvTxData into the stream buffer.
    ///     TicksToWait      : The maximum amount of time the task should remain in the Blocked state to wait for enough space to become available in the stream
    ///         buffer, should the stream buffer contain too little space to hold the another xDataLengthBytes bytes.  The block time is specified in tick periods,            
    ///         so the absolute time it represents is dependent on the tick frequency.  The macro pdMS_TO_TICKS() can be used to convert a time specified in milliseconds
    ///         into a time specified in ticks.  Setting xTicksToWait to portMAX_DELAY will cause the task to wait indefinitely (without timing out), provided
    ///         INCLUDE_vTaskSuspend is set to 1 in FreeRTOSConfig.h.  If a task times out before it can write all xDataLengthBytes into the buffer it will still write
    ///         as many bytes as possible.  A task does not use any CPU time when it is in the blocked state.
    ///
    /// * The Resource Code : stream_buffer.c line 764-865
    /// * Return
    ///     The number of bytes written to the stream buffer.  If a task times out before it can write all xDataLengthBytes into the buffer it will still
    ///     write as many bytes as possible.
    pub fn StreamBufferReceive(
        &mut self,
        RxData: &mut u8,
        BufferLengthBytes: UBaseType,
        TicksToWait: TickType,
    ) -> UBaseType {

        let mut ReceiveLength: UBaseType = 0;
        let mut BytesAvailable: UBaseType;
        let mut BytesToStoreMessageLength: UBaseType;

        if (self.get_flag() & sbFLAGS_IS_MESSAGE_BUFFER) != 0 {
            BytesToStoreMessageLength = sbBYTES_TO_STORE_MESSAGE_LENGTH!();
        } else {
            BytesToStoreMessageLength = 0;
        }

        if TicksToWait != 0 {
            taskENTER_CRITICAL!();
            BytesAvailable = self.BytesInBuffer();

            if BytesAvailable <= BytesToStoreMessageLength {
                //( void ) xTaskNotifyStateClear( NULL );

                // assert!(unwrap_streambuffer.xTaskWaitingToReceive == NULL);
                get_streambuffer_from_handle_mut!(self).xTaskWaitingToReceive = Some(get_current_task_handle!());
            } else {
                mtCOVERAGE_TEST_MARKER!();
            }
            taskEXIT_CRITICAL!();

            if BytesAvailable <= BytesToStoreMessageLength {
                traceBLOCKING_ON_STREAM_BUFFER_RECEIVE!(unwrap_streambuffer);

                //TaskNotifyWait
                taskENTER_CRITICAL!();
                {
                    if get_current_task_handle!().get_notify_state() != taskNOTIFICATION_RECEIVED!()
                    {
                        get_current_task_handle!().set_notify_state(taskWAITING_NOTIFICATION!());

                        if TicksToWait > 0 {
                            add_current_task_to_delayed_list(TicksToWait, true);
                            traceTASK_NOTIFY_WAIT_BLOCK!();

                            portYIELD_WITHIN_API!();
                        } else {
                            mtCOVERAGE_TEST_MARKER!();
                        }
                    } else {
                        mtCOVERAGE_TEST_MARKER!();
                    }
                }
                taskEXIT_CRITICAL!();

                taskENTER_CRITICAL!();
                {
                    traceTASK_NOTIFY_WAIT!();

                    get_current_task_handle!().set_notify_state(taskNOT_WAITING_NOTIFICATION!());
                }
                taskEXIT_CRITICAL!();
                // TaskNotifyWait

                get_streambuffer_from_handle_mut!(self).xTaskWaitingToReceive = None;
                BytesAvailable = self.BytesInBuffer();
            } else {
                mtCOVERAGE_TEST_MARKER!();
            }
        } else {
            BytesAvailable = self.BytesInBuffer();
        }

        let mut tmp:u8;
        // let mut unwrap_streambuffer = get_streambuffer_from_handle_mut!(self);

        if BytesAvailable > BytesToStoreMessageLength {
            ReceiveLength =
                self.ReadMessageFromBuffer(RxData, BufferLengthBytes, BytesAvailable);

            if ReceiveLength != 0 {
                // receive_completed!(unwrap_streambuffer);
            } else {
                mtCOVERAGE_TEST_MARKER!();
            }
        } else {
            traceSTREAM_BUFFER_RECEIVE_FAILED!();
            mtCOVERAGE_TEST_MARKER!();
        }

        ReceiveLength
    }

    /// A function to help write a stream buffer to a task.
    ///
    /// * Implemented by Chen Wenjie
    /// * Arguments
    ///     self             : The handle of the stream buffer to which a stream is being sent.
    ///     TxData           : A pointer to the buffer that holds the bytes to be copied into the stream buffer.
    ///     DataLengthBytes  : The maximum number of bytes to copy from pvTxData into the stream buffer.
    ///     Space            : The available space in the stream buffer
    ///     RequireSpace     : The space needed to write all data
    ///
    /// * The Resource Code : stream_buffer.c 721-760
    /// * Return
    ///     The number of bytes written to the stream buffer.
    // fn WriteMessageToBuffer(
    //     &self,
    //     TxData: u8,
    //     mut DataLengthBytes: UBaseType,
    //     Space: UBaseType,
    //     RequiredSpace: UBaseType,
    // ) -> UBaseType {
    //     let mut unwrap_streambuffer = get_streambuffer_from_handle_mut!(self);

    //     let mut NextHead = unwrap_streambuffer.xHead;

    //     let bytesinstore = sbBYTES_TO_STORE_MESSAGE_LENGTH!();

    //     if (unwrap_streambuffer.ucFlag & sbFLAGS_IS_MESSAGE_BUFFER) != 0 {
    //         if Space >= RequiredSpace {
    //             unwrap_streambuffer.pucBuffer = TxData;
    //         } else {
    //             DataLengthBytes = 0;
    //         }
    //     } else {
    //         if DataLengthBytes >= Space {
    //             DataLengthBytes = Space;
    //         }
    //     }

    //     if DataLengthBytes != 0 {
    //         unwrap_streambuffer.pucBuffer = TxData;
    //     }

    //     DataLengthBytes
    // }

    /// A function to help read a stream buffer to a task.
    ///
    /// * Implemented by Chen Wenjie
    /// * Arguments
    ///     self             : The handle of the stream buffer to which a stream is being sent.
    ///     RxData           : A pointer to the buffer that holds the bytes to be read from the stream buffer.
    ///     BufferLengthBytes: The maximum number of bytes to copy from pvTxData into the stream buffer.
    ///     BytesAvailable   : The available space in the stream buffer
    ///
    /// * The Resource Code : stream_buffer.c 965-1014
    /// * Return
    ///     The number of bytes read from the stream buffer.
    fn ReadMessageFromBuffer(
        &self,
        RxData: &mut u8,
        BufferLengthBytes: UBaseType,
        mut BytesAvailable: UBaseType,
    ) -> UBaseType {
        let mut unwrap_streambuffer = get_streambuffer_from_handle_mut!(self);

        let mut Count: UBaseType;
        let mut NextMessageLength: UBaseType = 1;
        let mut TempNext: UBaseType;
        let mut NextTail: UBaseType = unwrap_streambuffer.xTail;

        if (unwrap_streambuffer.ucFlag & sbFLAGS_IS_MESSAGE_BUFFER) != 0 {
            *RxData = unwrap_streambuffer.pucBuffer;
            unwrap_streambuffer.xTail += sbBYTES_TO_STORE_MESSAGE_LENGTH!();

            BytesAvailable -= sbBYTES_TO_STORE_MESSAGE_LENGTH!();

            if NextMessageLength > BufferLengthBytes {
                NextMessageLength = 0;
            } else {
                mtCOVERAGE_TEST_MARKER!();
            }
        } else {
            NextMessageLength = BufferLengthBytes;
        }

        if NextMessageLength > BytesAvailable {
            Count = BytesAvailable;
        } else {
            Count = NextMessageLength;
        }

        if Count != 0 {
            *RxData = unwrap_streambuffer.pucBuffer;
            unwrap_streambuffer.xTail += sbBYTES_TO_STORE_MESSAGE_LENGTH!();
        }

        Count
    }

    pub fn BytesInBuffer(&self) -> UBaseType {
        get_streambuffer_from_handle!(self).BytesInBuffer()
    }
}

#[macro_export]
macro_rules! get_streambuffer_from_handle {
    ($handle: expr) => {
        match $handle.0.try_read() {
            Ok(a) => a,
            Err(_) => {
                warn!("Stream Buffer was locked, read failed");
                panic!("Stream Buffer handle locked!");
            }
        }
    };
}

#[macro_export]
macro_rules! get_streambuffer_from_handle_mut {
    ($handle: expr) => {
        match $handle.0.try_write() {
            Ok(a) =>  a,
            Err(_) => {
                warn!("Stream Buffer was locked, write failed");
                panic!("Stream Buffer handle locked!");
            }
        }
    };
}

#[macro_export]
macro_rules! get_streambuffer_handle_from_option {
    ($option: expr) => {
        match $option {
            Some(handle) => handle,
            None => TaskHandle::from(TCB::new()),
        }
    };
}

pub fn send_completed(stream_buffer: &mut StreamBufferDef) {
    task_suspend_all();

    if stream_buffer.xTaskWaitingToReceive.is_some() {
        taskENTER_CRITICAL!();

        // let task_receive = stream_buffer.xTaskWaitingToReceive;

        let  OriginalNotifyState = stream_buffer.clone().xTaskWaitingToReceive.unwrap().get_notify_state();


        traceTASK_NOTIFY!();

        if OriginalNotifyState == taskWAITING_NOTIFICATION!() {
            // list_remove(stream_buffer.clone().xTaskWaitingToReceive.unwrap().get_event_list_item());

            stream_buffer.clone()
                .xTaskWaitingToReceive
                .unwrap()
                .append_task_to_ready_list();

            if get_current_task_handle!().get_priority()
                < stream_buffer.clone().xTaskWaitingToReceive.unwrap().get_priority()
            {
                taskYIELD_IF_USING_PREEMPTION!();
            } else {
                mtCOVERAGE_TEST_MARKER!();
            }
        } else {
            mtCOVERAGE_TEST_MARKER!();
        }

        taskEXIT_CRITICAL!();

        stream_buffer.xTaskWaitingToReceive = None;
    }

    task_resume_all();
}

#[macro_export]
macro_rules! receive_completed {
    ($stream_buffer:expr) => {
        task_suspend_all();

        if $stream_buffer.xTaskWaitingToSend.is_some() {
            taskENTER_CRITICAL!();

            let OriginalNotifyState = $stream_buffer.clone()
                .xTaskWaitingToSend
                .unwrap()
                .get_notify_value();

            $stream_buffer.clone()
                .xTaskWaitingToSend
                .unwrap()
                .set_notify_value(taskNOTIFICATION_RECEIVED!());

            traceTASK_NOTIFY!();

            if OriginalNotifyState == taskWAITING_NOTIFICATION!() {
                list_remove(($stream_buffer.clone().xTaskWaitingToSend.unwrap()).get_event_list_item());

                $stream_buffer.clone()
                    .xTaskWaitingToSend
                    .unwrap()
                    .append_task_to_ready_list();

                if get_current_task_handle!().get_priority()
                    < $stream_buffer.clone().xTaskWaitingToSend.unwrap().get_priority()
                {
                    taskYIELD_IF_USING_PREEMPTION!();
                } else {
                    mtCOVERAGE_TEST_MARKER!();
                }
            } else {
                mtCOVERAGE_TEST_MARKER!();
            }

            taskEXIT_CRITICAL!();

            $stream_buffer.xTaskWaitingToSend = None;
        }

        task_resume_all();
    };
}

#[macro_export]
macro_rules! sbBYTES_TO_STORE_MESSAGE_LENGTH {
    () => {
        8
    };
}

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
        set_tick_count!(const_tick_count + 1);
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
            // task_set_time_out_state(pxtimeout);
            xreturn = false;
        } else {
            trace!("ELSE");
            xreturn = true;
        }
    }
    taskEXIT_CRITICAL!();

    xreturn
}
