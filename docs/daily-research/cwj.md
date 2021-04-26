# stream_buffer
## 宏定义
+ STREAM_BUFFER_H
+ INC_FREERTOS_H

## 结构体
+ StreamBufferDef_t

## 函数
+ xStreamBufferCreate
  原型:
  `StreamBufferHandle_t xStreamBufferCreate（size_t xBufferSizeBytes，size_t xTriggerLevelBytes）;`
  功能：
  使用静态分配的内存创建一个新的流缓冲区。
```
* void vAFunction( void )
 * {
 * StreamBufferHandle_t xStreamBuffer;
 * const size_t xStreamBufferSizeBytes = 100, xTriggerLevel = 10;
 *
 *  // Create a stream buffer that can hold 100 bytes.  The memory used to hold
 *  // both the stream buffer structure and the data in the stream buffer is
 *  // allocated dynamically.
 *  xStreamBuffer = xStreamBufferCreate( xStreamBufferSizeBytes, xTriggerLevel );
 *
 *  if( xStreamBuffer == NULL )
 *  {
 *      // There was not enough heap memory space available to create the
 *      // stream buffer.
 *  }
 *  else
 *  {
 *      // The stream buffer was created successfully and can now be used.
 *  }
```

+ xStreamBufferSend
  原型：
  `size_t  xStreamBufferSend（StreamBufferHandle_t xStreamBuffer,const  void * pvTxData，size_t xDataLengthBytes，TickType_t xTicksToWait）PRIVILEGED_FUNCTION;`
  功能：
  将字节发送到流缓冲区。字节被复制到流缓冲区中。

+ xStreamBufferSendFromISR
  原型：
  功能：
    API函数的中断安全版本，可将字节流发送到流缓冲区。
```
* StreamBufferHandle_t xStreamBuffer;
 *
 * void vAnInterruptServiceRoutine（void）
 * {
 * size_t xBytesSent;
 * char * pcStringToSend =“要发送的字符串”;
* BaseType_t xHigherPriorityTaskWoken = pdFALSE; //初始化为pdFALSE。
 *
 * //尝试将字符串发送到流缓冲区。
 * xBytesSent = xStreamBufferSendFromISR（xStreamBuffer，
 *（void *）pcStringToSend，
 * strlen（pcStringToSend），
 *＆xHigherPriorityTaskWoken）;
 *
 *如果（xBytesSent！= strlen（pcStringToSend））
 * {
 * //整个流缓冲区中没有足够的可用空间
 * //要写入的字符串，已写入ut xBytesSent字节。
 *}
 *
 * //如果内部将xHigherPriorityTaskWoken设置为pdTRUE
 * // xStreamBufferSendFromISR（），然后执行优先级高于
 * //当前执行的任务的优先级已解除阻止，并且上下文
 * //应该执行切换以确保ISR返回到畅通无阻的状态
* // 任务。在大多数FreeRTOS端口中，这是通过简单地传递来完成的
 * //将xHigherPriorityTask唤醒到taskYIELD_FROM_ISR（）中，这将测试
* //变量值，并在必要时执行上下文切换。检查
 * //有关正在使用的端口的文档，以获取特定于端口的说明。
 * taskYIELD_FROM_ISR（xHigherPriorityTaskWoken）;
 *}
```
+ xStreamBufferReceive
  功能：
  从流缓冲区接收字节。

+ xStreamBufferReceiveFromISR
  功能：从流缓冲区读取字节（来自中断服务程序

+ vStreamBufferDelete
  功能：删除流

+ xStreamBufferIsFull
+ xStreamBufferIsEmpty
+ xStreamBufferSpacesAvailable

+ xStreamBufferReset
  
+ xStreamBufferSetTriggerLevel
  触发等级：流缓冲区的触发级别是在流缓冲区上阻止的任务离开阻止状态之前，必须在流缓冲区中的字节数。例如，如果任务在读取触发器级别为1的空流缓冲区时被阻止，则当单个字节写入缓冲区或任务的阻止时间过期时，该任务将被取消阻止。作为另一个示例，如果任务在读取触发器级别为10的空流缓冲区时被阻止，则在流缓冲区包含至少10个字节或任务的阻止时间到期之前，任务不会被解除阻止。如果读取任务的块时间在达到触发器级别之前过期，那么无论实际可用的字节数是多少，该任务仍将接收。将触发器级别设置为0将导致使用触发器级别1。指定大于缓冲区大小的触发器级别无效。