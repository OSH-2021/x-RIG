# 简介
## 什么是实时操作系统

大多数操作系统似乎允许多个程序同时执行。这称为多任务处理。实际上，每个处理器内核只能在任何给定的时间点运行单个执行线程。操作系统的一部分称为调度程序，负责确定何时运行哪个程序，并通过在每个程序之间快速切换来提供同时执行的错觉。
操作系统的类型由调度程序决定何时运行哪个程序来定义。例如，在多用户操作系统（例如Unix）中使用的调度程序将确保每个用户获得相当数量的处理时间。作为另一个示例，桌面操作系统（例如Windows）中的调度程序将尝试确保计算机保持对用户的响应。

实时操作系统（RTOS）中的调度程序旨在提供可预测的（通常描述为确定性的）执行模式。由于嵌入式系统通常具有实时要求，因此嵌入式系统尤其需要注意这一点。实时要求是一种要求嵌入式系统必须在严格定义的时间（最后期限）内响应特定事件的要求。只有在可以预测操作系统调度程序的行为（因此是确定性的）的情况下，才能保证满足实时要求。

传统的实时调度程序（例如FreeRTOS中使用的调度程序）通过允许用户为每个执行线程分配优先级来实现确定性。然后，调度程序使用优先级来知道接下来要运行哪个执行线程。在FreeRTOS中，执行线程称为任务。

## 什么是FreeRTOS

FreeRTOS是一类RTOS，其大小设计得足以在微控制器上运行-尽管它的使用不仅限于微控制器应用。

微控制器是一种受资源限制的小型处理器，在单个芯片上集成了处理器本身，只读存储器（ROM或闪存）以保存要执行的程序以及程序所需要的随机存取存储器（RAM）执行。通常，该程序直接从只读存储器执行。

单片机用于深度嵌入的应用程序（那些您从未真正看到过处理器本身或正在运行的软件的应用程序）中，这些应用程序通常具有非常专门的工作。大小限制和专用的最终应用程序性质很少保证使用完整的RTOS实现-或确实使使用完整的RTOS实现成为可能。**因此，FreeRTOS仅提供核心的实时调度功能，任务间通信，定时和同步原语。这意味着它可以更准确地描述为实时内核或实时执行程序。然后，附加组件可以包含其他功能，例如命令控制台界面或网络堆栈**

## 多任务
传统处理器一次只能执行一个任务-但是通过在任务之间快速切换，多任务操作系统可以使其看起来好像每个任务都在同时执行。下图描述了这三个任务相对于时间的执行模式。任务名称用颜色编码并写在左侧。时间从左向右移动，彩色线显示在任何特定时间正在执行的任务。上面的图展示了感知到的并发执行模式，下面的图展示了实际的多任务执行模式。
![](https://www.freertos.org/fr-content-src/uploads/2018/07/TaskExecution.gif)

## 调度
实时操作系统（RTOSes）使用这些相同的原理实现多任务处理-但是它们的目标与非实时系统的目标有很大不同。不同的目标反映在调度策略中。实时/嵌入式系统旨在提供对现实事件的及时响应。现实世界中发生的事件可以有一个截止时间，在这个截止时间之前，实时/嵌入式系统必须做出响应，并且RTOS调度策略必须确保这些截止时间得到满足。
为了实现此目标，软件工程师必须首先为每个任务**分配优先级**。然后，RTOS的调度策略是简单地确保能够执行的最高优先级任务是给定处理时间的任务。如果它们准备同时运行，则可能需要在优先级相同的任务之间“公平地”共享处理时间。

# 历史记录
## 可能可行的迭代选择
+ Task notifications（10.3.1~10.4）
+ Kernel ports that support memory protection units (MPUs)（10.3.1~10.4）
+  Added the vPortGetHeapStats() API function which returns information on
	  the heap_4 and heap_5 state.
+ Added xTaskCatchUpTicks(), which corrects the tick count value after the
	  application code has held interrupts disabled for an extended period.
+ Added xTaskNotifyValueClear() API function.
+ Added uxTimerGetReloadMode() API function
+ Add vTimerSetReloadMode(), xTaskGetIdleRunTimeCounter(), and xTaskGetApplicationTaskTagFromISR() API functions.
+ Added uxTaskGetStackHighWaterMark2() function to enable the return type to be changed without breaking backward compatibility. uxTaskGetStackHighWaterMark() returns a UBaseType_t as always, uxTaskGetStackHighWaterMark2() returns configSTACK_DEPTH_TYPE to allow the user to determine the return type.
+ Stream Buffers - see https://www.FreeRTOS.org/RTOS-stream-buffer-example.html
+ Message Buffers - see https://www.FreeRTOS.org//RTOS-message-buffer-example.html

# 代码粗比对
+ 代码风格变化
+ 注释变化
+ 命名变化
+ 新增stream_buffer.c

# Message Buffers
+ 流缓冲区是RTOS任务到RTOS任务的中断，是对任务通信原语的中断。与大多数其他FreeRTOS通信原语不同，它们针对单读取器单写入器场景进行了优化，例如将数据从中断服务例程传递到任务，或从一个微控制器内核传递到双核CPU上的另一个内核。数据通过复制传递-发送者将数据复制到缓冲区中，并通过读取将其复制到缓冲区之外。
流缓冲区传递连续的字节流。消息缓冲区传递可变大小但不连续的消息。消息缓冲区使用流缓冲区进行数据传输。

+ FreeRTOS消息缓冲区和流缓冲区提供了为队列提供更小，更快的替代方案。

+ 流缓冲区允许将字节流从中断服务例程传递到任务，或从一个任务传递到另一任务。字节流可以具有任意长度，并且不一定具有开头或结尾。可以一次写入任意数量的字节，并且可以一次读取任意数量的字节。数据通过复制传递-发送者将数据复制到缓冲区中，并通过读取将其复制到缓冲区之外。与大多数其他FreeRTOS通信原语不同，流缓冲区针对单读取器单写入器场景进行了优化，例如将数据从中断服务例程传递到任务，或从一个微控制器内核传递到双核CPU上的另一个。

+ 消息缓冲区允许将可变长度的离散消息从中断服务例程传递到任务，或从一个任务传递到另一任务。例如，长度为10、20和123字节的消息都可以写入和读取同一消息缓冲区。与使用流缓冲区不同，10字节消息只能作为10字节消息而不是单个字节读出。消息缓冲区建立在流缓冲区之上（也就是说，它们使用流缓冲区实现）。数据通过复制传递到消息缓冲区中-发送方将数据复制到缓冲区中，并通过读取将数据复制出缓冲区。

+ 如果在任务使用xMessageBufferReceive（）从碰巧为空的消息缓冲区中读取时指定了非零的阻止时间，则该任务将被置于“阻止”状态（因此，它不会消耗任何CPU时间，并且其他任务可以运行）直到消息缓冲区中的数据可用或阻止时间到期为止。

如果在任务使用xMessageBufferSend（）写入恰好已满的消息缓冲区时指定了非零的阻止时间，则该任务将被置于“阻止”状态（因此它不会消耗任何CPU时间，其他任务也可以运行）直到消息缓冲区中的任何空间变为可用，或者阻止时间到期为止。

# Task Notifications
## 什么是直接任务通知？
大多数任务间通信方法都通过中介对象，例如队列，信号量或事件组。发送任务写入通信对象，接收任务从通信对象读取。顾名思义，当使用直接任务通知时，发送任务将通知直接发送给接收任务，而无需中间对象。

从FreeRTOS V10.4.0开始，每个任务都有一系列通知。在此之前，每个任务都有一个通知。每个通知都包含一个32位值和一个布尔状态，它们一起仅消耗5个字节的RAM。

正如任务可以阻止二进制信号量以等待该信号量变为“可用”一样，任务可以阻止通知以等待该通知的状态变为“待处理”。同样，就像任务可以阻止计数信号量以等待该信号量的计数变为非零一样，任务可以阻止通知以等待该通知的值变为非零。下面的第一个示例演示了这种情况。

# Refference
![](https://www.freertos.org/fr-content-src/uploads/2020/09/Drawing1.png)
![](https://www.freertos.org/fr-content-src/uploads/2020/09/Drawing2.png)
[freertos官方文档](https://www.freertos.org/)
[freertos-GitHub](https://github.com/FreeRTOS/FreeRTOS-Kernel)


```
rustup install nightly
rustup default nightly
cargo install cc
cargo install bindgen
sudo apt install clang
```