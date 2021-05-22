# 5.22
PRIVILEGED_FUNCTION

PRIVILEGED_FUNCTION 这个宏是用于存储保护单元芯片的。

这几个任务创建函数都是用于任务创建，任务一旦创建就会被插入任务就绪链表中，当调度器调度启动后就按任务状态机根据调度策略以及外部输入事件进行调度接管。

StreamBufferDef_t

## xStreamBufferCreate( xBufferSizeBytes, xTriggerLevelBytes ) 

xStreamBufferCreateStatic( xBufferSizeBytes, xTriggerLevelBytes, pucStreamBufferStorageArea, pxStaticStreamBuffer )
在流缓冲区上被阻塞的任务等待数据移出阻塞状态之前，流缓冲区中必须存在的字节数。例如，如果任务在读取触发器级别为1的空流缓冲区时被阻止，则当单个字节写入缓冲区或任务的阻止时间过期时，该任务将被取消阻止。作为另一个示例，如果任务在读取触发器级别为10的空流缓冲区时被阻止，则在流缓冲区包含至少10个字节或任务的阻止时间到期之前，任务不会被解除阻止。如果读取任务的块时间在达到触发器级别之前过期，那么无论实际可用的字节数是多少，该任务仍将接收。将触发器级别设置为0将导致使用触发器级别1。指定大于缓冲区大小的触发器级别无效。

## xStreamBufferSend( StreamBufferHandle_t xStreamBuffer, const void *pvTxData, size_t xDataLengthBytes, TickType_t xTicksToWait );
如果流缓冲区包含的空间太小，无法容纳另一个xDataLengthBytes，则任务应保持在阻止状态以等待流缓冲区中有足够空间可用的最长时间。阻塞时间是以滴答周期指定的，因此它所代表的绝对时间取决于滴答频率。宏pdMS TO_TICKS（）可用于将以毫秒为单位指定的时间转换为以TICKS为单位指定的时间。如果在FreeRTOSConfig.h中将INCLUDE\vTaskSuspend设置为1，那么将xTicksToWait设置为portMAX\u DELAY将导致任务无限期等待（不超时）。如果任务在将所有xDataLengthBytes写入缓冲区之前超时，它仍将尽可能多地写入字节。任务处于阻塞状态时不使用任何CPU时间。
## size_t xStreamBufferSendFromISR( StreamBufferHandle_t xStreamBuffer, const void *pvTxData, size_t xDataLengthBytes, BaseType_t *pxHigherPriorityTaskWoken );
流缓冲区上可能有一个任务被阻塞，等待数据。调用xStreamBufferSendFromISR（）可以使数据可用，从而导致等待数据的任务离开阻塞状态。如果调用xStreamBufferSendFromISR（）导致任务离开阻塞状态，并且未阻塞的任务的优先级高于当前正在执行的任务（被中断的任务），那么在内部，xStreamBufferSendFromISR（）会将*pxHigherPriorityTaskWoken设置为pdTRUE。如果xStreamBufferSendFromISR（）将此值设置为pdTRUE，则通常应在中断退出之前执行上下文切换。这将确保中断直接返回到最高优先级就绪状态任务 *pxHigherPriorityTaskWoken在传递到函数之前应设置为pdFALSE。有关示例，请参

## size_t xStreamBufferReceive( StreamBufferHandle_t xStreamBuffer,void * pvRxData,size_t xBufferLengthBytes,TickType_t xTicksToWait ) PRIVILEGED_FUNCTION;

## size_t xStreamBufferReceiveFromISR( StreamBufferHandle_t xStreamBuffer, void *pvRxData, size_t xBufferLengthBytes, BaseType_t *pxHigherPriorityTaskWoken );

## void vStreamBufferDelete( StreamBufferHandle_t xStreamBuffer ) PRIVILEGED_FUNCTION;


##  void vStreamBufferDelete( StreamBufferHandle_t xStreamBuffer ) PRIVILEGED_FUNCTION;

## BaseType_t xStreamBufferIsFull( StreamBufferHandle_t xStreamBuffer ) PRIVILEGED_FUNCTION;

## BaseType_t xStreamBufferIsEmpty( StreamBufferHandle_t xStreamBuffer ) PRIVILEGED_FUNCTION;

## BaseType_t xStreamBufferReset( StreamBufferHandle_t xStreamBuffer ) PRIVILEGED_FUNCTION;
A stream buffer can only be reset if there are no tasks blocked waiting to either send to or receive from the stream buffer.

##  size_t xStreamBufferSpacesAvailable( StreamBufferHandle_t xStreamBuffer ) PRIVILEGED_FUNCTION;
Queries a stream buffer to see how much free space it contains, which is equal to the amount of data that can be sent to the stream buffer before it is full.

## size_t xStreamBufferBytesAvailable( StreamBufferHandle_t xStreamBuffer ) PRIVILEGED_FUNCTION;

##  BaseType_t xStreamBufferSetTriggerLevel( StreamBufferHandle_t xStreamBuffer, size_t xTriggerLevel ) PRIVILEGED_FUNCTION;
流缓冲区的触发级别是在流缓冲区上被阻止以等待数据移出阻止状态的任务之前，必须在流缓冲区中的字节数。例如，如果任务在读取触发器级别为1的空流缓冲区时被阻止，则当单个字节写入缓冲区或任务的阻止时间过期时，该任务将被取消阻止。作为另一个示例，如果任务在读取触发器级别为10的空流缓冲区时被阻止，则在流缓冲区包含至少10个字节或任务的阻止时间到期之前，任务不会被解除阻止。如果读取任务的块时间在达到触发器级别之前过期，那么无论实际可用的字节数是多少，该任务仍将接收。将触发器级别设置为0将导致使用触发器级别1。指定大于缓冲区大小的触发器级别无效。


##  BaseType_t xStreamBufferSendCompletedFromISR( StreamBufferHandle_t xStreamBuffer,BaseType_t * pxHigherPriorityTaskWoken ) PRIVILEGED_FUNCTION;
当数据被发送到消息缓冲区或流缓冲区时，sbSEND_COMPLETED（）宏从FreeRTOS api中调用。如果消息或流缓冲区中有一个任务被阻止，等待数据到达，那么sbSEND_COMPLETED（）宏会向该任务发送通知，将其从阻止状态中删除。xstreamBufferEndCompletedFromIsr（）做同样的事情。它的提供使应用程序编写器能够实现自己的sbSEND_COMPLETED（）版本，并且不能在任何其他时间使用。


# 5.19
## kernel
.taskYIELD()
  比如我创建了8个优先级一样的task,并且没有创建其他优先级的进程,
  而且8个task每个task都不会调用任何引起本task从就绪运行队列链表中被摘掉的系统函数,就像示例中
  vStartIntegerMathTasks()创建vCompeteingIntMathTask1(),vCompeteingIntMathTask2()...vCompeteingIntMathTask8()一样,
  每个task都是不会睡眠的不停的执行自己,当每个task觉得自己占用cpu的时间已经差不多的时候,
  就会调用taskYIELD(),主动让出cpu,让同优先级的其他task获得cpu,因为没有其他优先级的task,所以调度器不会切换优先级,

  而是采用轮转调度策略,运行同优先级的就绪运行队列链表中调用taskYIELD()函数的当前task的下一个task.

  就这样8个task轮流让出cpu给同优先级的下一个兄弟task,8个task都采用主动协作的方式,彼此安全顺利的跑了起来.

PENDING_READY_LIST


## before
+ task状态：runing, ready, blocked, suspend, delete, invalid
+ task 通信状态： no action, set bits, increment, set a value with overwrite, set a value without overwrite

BaseType_t xTaskCreate( TaskFunction_t pxTaskCode,
      const char * const pcName,     /*lint !e971 Unqualified char types are allowed for strings and single characters only. */
      const configSTACK_DEPTH_TYPE usStackDepth,
      void * const pvParameters,
      UBaseType_t uxPriority,
      TaskHandle_t * const pxCreatedTask ) PRIVILEGED_FUNCTION;

# rust
+ 状态：无invalid
+ 结构体：tcb
  > name(), stacksize(), priority(), initialise
+ Default trait


## task 组成
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

## task api
+ new 
+ name： 命名
+ stacksize ：
+ PartialEq
+ 

### TCB/Task
#### 参数
state_list_item   event_list_item    task_priority    task_stacksize    task_name   stack_pos    critical_nesting   base_priority    mutexes_held    runtime_counter      notified_value    notify_state   delay_aborted
#### 方法
new  get_state_list_item   get_event_list_item    get_priority    set_priority   get_name   get_run_time    set_run_time...

From

#### TaskHandle

#### 方法
from_arc   from    set_priority




  

## Q
+ RwLock
+ lazy_static

## task 官方文档

+ 任务状态
> The Blocked State
>> Tasks can enter the Blocked state to wait for two different types of event:
>> 1. 时态（与时间相关的）事件是指延迟期到期或达到绝对时间的事件。例如，任务可能会进入阻塞状态，等待10毫秒才能通过。
>> 2. 事件源自另一个任务或中断的同步事件。例如，任务可能会进入阻塞状态，等待数据到达队列。同步事件涵盖了广泛的事件类型。
>> FreeRTOS队列、二进制信号量、计数信号量、互斥量、递归互斥量、事件组和直接到任务的通知都可以用于创建同步事件。所有这些功能都将在本书的后续章节中介绍。任务有可能在同步事件上阻塞超时，有效地同时阻断两种类型的事件。例如，任务可能会选择等待最长10毫秒，以便数据到达队列。如果数据在10毫秒内到达，或者10毫秒后没有数据到达，任务将离开阻塞状态。

> The Suspended State
>>“暂停”也是不运行的子状态。处于挂起状态的任务对计划程序不可用。进入挂起状态的唯一方法是调用vtasksuspend（）API函数，唯一的出路是调用vTaskResume（）或xTaskResumeFromISR（）API函数。大多数应用程序不使用挂起状态。

>The Ready State
>>处于未运行状态但未被阻止或挂起的任务称为处于就绪状态。它们能够运行，因此“准备好”运行，但当前未处于运行状态。

>Completing the State Transition Diagram
>>图15扩展了之前过于简化的状态图，包括本节中描述的所有未运行的子状态。到目前为止，示例中创建的任务没有使用阻塞或挂起状态；它们只在就绪状态和运行状态之间转换，运行状态由图15中粗体的行突出显示。
![](./1.png)

+ IDLE WORK & IDLE HOOK

+ 优先级 查看与更改

+ 调度算法
  +  A Recap of Task States and Events
  +  Configuring the Scheduling Algorithm
  +  Prioritized Pre-emptive Scheduling with Time Slicing
  +  Prioritized Pre-emptive Scheduling (without Time Slicing)
  +  Co-operative Scheduling> 


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