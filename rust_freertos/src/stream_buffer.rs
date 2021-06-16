use crate::port::*;
use crate::task_control::*;
use std::fmt;


#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub enum StreamBufferError {
    StreamBufferTriggerLevelOverflow,
    StreamBufferFull,
    StreamBufferEmpty,
}
impl fmt::Display for StreamBufferError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            StreamBufferError::StreamBufferTriggerLevelOverflow => write!(f, "TriggerLevelOverflow"),
            StreamBufferError::StreamBufferFull => write!(f, "StreamBufferFull"),
            StreamBufferError::StreamBufferEmpty => write!(f, "StreamBufferEmpty"),
        }
    }
}

pub const sbFLAGS_IS_MESSAGE_BUFFER : u8 = 1;
pub const sbFLAGS_IS_STATICALLY_ALLOCATED : u8 = 2;
/// *Descrpition:
/// Definition of  Stream Buffer
/// 
/// 
/// *Implemented by Chen Wenjie
pub struct StreamBufferDef{
    xTail:UBaseType,
    xHead:UBaseType,
    xLength:UBaseType,
    xTriggerLevelBytes:UBaseType,
    xStreamBufferWaitingToReceive:StreamBufferHandle,
    xStreamBufferWaitingToSend:StreamBufferHandle,
    pucBuffer:Weak<RwLock<u8>>,
    ucFlag:u8,

    #[cfg(feature = "configUSE_TRACE_FACILITY")]
    uxStreamBufferNumber:UBaseType
}


impl StreamBufferDef{



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
    ///     The created stream buffer
    pub fn StreamBufferGenericCreate (xBufferSizeBytes:UBaseType, xTriggerLevelBytes:UBaseType, xIsMessageBuffer:bool, pucStreamBufferStorageArea:Weak<RwLock<u8>>) -> Self{

        let mut ucFlag:u8;
        let mut streambuffer : StreamBufferDef;
        if xIsMessageBuffer == true {

            ucFlags = sbFLAGS_IS_MESSAGE_BUFFER;

        }
        else{

            ucFlag = 0;
        }

        if xTriggerLevelBytes ==  0 {
            xTriggerLevelBytes == 1;
        }

        streambuffer.InitialiseNewStreamBuffer(pucStreamBufferStorageArea, xBufferSizeBytes, xTriggerLevelBytes, ucFlag);

        streambuffer

    }



    /// Reset a stream buffer
    /// * Implemented by Chen Wenjie
    /// * Arguments
    ///     self             : The handle of the stream buffer to be reset.
    ///
    /// * Return
    /// 
    pub fn StreamBufferReset(&mut self) -> Result<(), StreamBufferError>{
    
        #[cfg(feature = "configUSE_TRACE_FACILITY")]
        uxStreamBufferNumber:UBaseType;

        #[cfg(feature = "configUSE_TRACE_FACILITY")]
        uxStreamBufferNumber = self.uxStreamBufferNumber;

        taskENTER_CRITICAL!();

        if self.xStreamBufferWaitingToReceive == NULL && self.xStreamBufferWaitingToSend == NULL {
            self.InitialiseNewStreamBuffer(self.pucBuffer, self.xLength, self.xTriggerLevelBytes, self.ucFlag);
            
            
            #[cfg(feature = "configUSE_TRACE_FACILITY")]
            self.uxStreamBufferNumber = uxStreamBufferNumber;
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
    /// * Return
    ///     
    pub fn StreamBufferSetTriggerLevel(&mut self, xTriggerLevel: UBaseType) -> Result<(), StreamBufferError>{
        

        if xTriggerLevelBytes ==  0 {
            xTriggerLevelBytes == 1;
        }

        if xTriggerLevel <= self.xLength {
            self.xTriggerLevelBytes = xTriggerLevel;
            return Ok(());
        }
        
        Err(StreamBufferTriggerLevelOverflow)
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
    pub fn StreamBufferBytesAvailable(&mut self) -> UBaseType{
        let mut xSpace : UBaseType;

        xSpace = self.xLength + self.xTail;
        xSpace -= (self.xHead + 1 as UBaseType);

        if xSpace >= self.xLength {

            xSpace -= self.xLength;
        
        }
        else {

            mtCOVERAGE_TEST_MARKER!();

        }
        xSpcae
    }


    /// Queries a stream buffer to see how much data it contains, which is equal to the number of bytes that 
    /// can be read from the stream buffer before the stream buffer would be empty.
    /// 
    /// * Implemented by Chen Wenjie
    /// * Arguments
    ///     self             : The handle of the stream buffer being queried.
    ///     xTriggerLevel    : The new trigger level for the stream buffer.
    /// * Return
    ///     The number of bytes that can be read from the stream buffer before the stream buffer would be empty.
    pub fn StreamBufferBytesAvailable(&mut self) -> UBaseType{

        self.BytesInBuffer()

    }




}

pub struct StreamBufferHandle(Arc<RwLock<StreamBufferDef>>);

impl From<Weak<RwLock<StreamBufferDef>>>{
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

impl StreamBufferHandle{
    pub fn from_arc(arc: Arc<RwLock<StreamBufferDef>>) -> Self {
        StreamBufferHandle(arc)
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
    pub fn from(stream_buffer: StreamBufferDef) -> Self {
        // TODO: Implement From.
        StreamBufferHandle(Arc::new(RwLock::new(stream_buffer)))
    }

    /* This function is for use in FFI. */
    pub fn as_raw(self) -> ffi::xStreamBufferHandle {
        Arc::into_raw(self.0) as *mut _
    }  
    
    
    


    /// Write to a stream buffer from a task.
    /// 
    /// * Implemented by Chen Wenjie
    /// * Arguments
    ///     self             : The handle of the stream buffer to which a stream is being sent.
    ///     pvTxData         : A pointer to the buffer that holds the bytes to be copied into the stream buffer.
    ///     xDataLengthBytes : The maximum number of bytes to copy from pvTxData into the stream buffer.
    ///     xTicksToWait     : The maximum amount of time the task should remain in the Blocked state to wait for enough space to become available in the stream
    ///         buffer, should the stream buffer contain too little space to hold the another xDataLengthBytes bytes.  The block time is specified in tick periods,            
    ///         so the absolute time it represents is dependent on the tick frequency.  The macro pdMS_TO_TICKS() can be used to convert a time specified in milliseconds
    ///         into a time specified in ticks.  Setting xTicksToWait to portMAX_DELAY will cause the task to wait indefinitely (without timing out), provided
    ///         INCLUDE_vTaskSuspend is set to 1 in FreeRTOSConfig.h.  If a task times out before it can write all xDataLengthBytes into the buffer it will still write
    ///         as many bytes as possible.  A task does not use any CPU time when it is in the blocked state.
    /// 
    /// * Return
    ///     The number of bytes written to the stream buffer.  If a task times out before it can write all xDataLengthBytes into the buffer it will still
    ///     write as many bytes as possible.
    





    


    



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
            Ok(a) => a,
            Err(_) => {
                warn!("Stream Buffer was locked, write failed");
                panic!("Stream Buffer handle locked!");
            }
        }
    };
}

