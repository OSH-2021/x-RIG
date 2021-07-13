# 结题报告
- [结题报告](#结题报告)
  - [项目简介](#项目简介)
  - [背景和立项依据](#背景和立项依据)
    - [项目背景](#背景)
    - [立项依据](#立项依据)
      - [版本更新](#版本更新)
      - [seL-4](#sel-4)
        - [seL4 Capability与FreeRTOS的关系](#sel4-capability与freertos的关系)
        - [CSpace](#cspace)
        - [Threads](#threads)
          - [scheduling contexts](#scheduling-contexts)
          - [异常](#异常)
        - [Notification](#notification)
        - [Fault](#fault)
          - [Capability Fault](#capability-fault)
          - [未知系统调用](#未知系统调用)
          - [用户异常](#用户异常)
          - [调试](#调试)
          - [Page Fault](#page-fault)
        - [中断](#中断)
        - [I/O](#io)
      - [Rust](#Rust)
  - [设计思路](#设计思路)
    - [stream buffer](#stream_buffer)
    - [capability](#capability)
  - [成果演示](#成果展示)
    - [stream buffer](#stream_buffer)
  - [总结](#总结)
    - [项目特色](#项目特色)
    - [反思](#反思)
    - [展望](#展望)
  - [致谢](#致谢)
  - [参考文献](#参考文献)

## 项目简介
RIG小组完成基于rust-freertos的版本更迭和改进，尝试使rust-freertos能够更好地用于生产生活中。

## 背景和立项依据
### 项目背景
+ 2019年，rust-freertos小组完成了对freertos的rust改写，但已经过去两年时间，freertos版本已经发生了较大的改变，我们小组通过全面学习freertos框架，在rust-freertos的基础上进行了版本更迭。
+ 2020年5月7日工信部发布《关于深入推进移动物联网全面发展的通知》，提出建立NB-IoT（窄带物联网）、4G和5G协同发展的移动物联网综合生态体系。作为实时性嵌入式操作系统市场占有率前三的freertos也拥有广泛的应用前景。
+ 在调研过程中，我们发现freertos缺乏一定的安全性保护，而同为实时性操作系统的sel-4却已经完成了形式化安全性验证，因此我们想要融入一部分sel-4的安全特性到freertos，以期望增强其安全性。

### 版本更新
2019年rust-freertos小组完成9.0版本的rust改写，而在2021年freertos已经推出了10.0版本，新增加了一些模块，我们完成了streambuffer的rust的改写

### seL-4
##### seL4 Capability与FreeRTOS的关系
[research.md](research.md)中介绍了seL4的kernel object以及capbilities

其中各个object大致对应的FreeRTOS部分为

| seL4 object               | FreeRTOS part                                                     |
| ------------------------- | ----------------------------------------------------------------- |
| Endpoints & Reply objects | queue&semaphore(queue.rs, queue_h.rs, queue_api.rs, semaphore.rs) |
| Thread Control Block(TCB) | task control block(task_control.rs)                               |
| scheduling context        | task schedule(kernel.rs)                                          |
| Interrupt object          | queue, C library                                                  |
| Notification              | Notification(目前未实现)                                          |
| CSpace                    | 目前未实现                                                        |


> 其他如Untyped Memory和CNode为capability机制特有的，或者FreeRTOS不存在这部分，如VSpace

对于FreeRTOS中的TCB，主要涉及数据结构的改写和CSpace, IPC buffer的添加

以下如果没有特殊说明，都是在seL4中的实现

seL4中主要采用线程的概念，与FreeRTOS采用的task类似
##### cspace
seL4通过在满足`kernel`的内存需求之后，通过`Untyped Memory object`分配给`initial thread`，然后之后的子线程可以通过`retype untyped memory`来实现object类型的转换


`Object size`对于`CNode`和`Untyped Memory`是可变的，而对于其他是固定长度的，在libsel4中定义，也就是说对于大部分object，其占用内存大小都是固定的。对于线程来说，其拥有的内存大小也是固定的，这不同于FreeRTOS的动态分配

下图是object进行分配的示意图

![](files/seL4/seL4_CDT.png)

`seL4_CNode_Delete()`删除一个cap，在只剩最后一个cap时删除整个CNode，内存会被释放，可以重用

`seL4_CNode_Revoke()`Delete所有CDT children的相应capability，最后一个会有相应的删除object操作

在FreeRTOS中需要通过单独开辟新的文件进行CSpace数据结构和方法的编写(x-qwq, cspace.rs)

#### Threads

MCS configuration | SMP configuration of the kernel

TCB(thread control block)
- CSpace & VSpace(shared with other thread)
- IPC buffer to transfer caps

##### scheduling contexts

-   (budgets, period) - (b, p)
-   RR scheduling
    -   budget charged each time the current node's scheduling context changed

-   b == p threads are treated as robin threads

passive thread没有scheduling contexts

##### 异常

分为标准异常和超时异常

标准异常需要`standard exception handler`来处理

超时异常可有可无，超时之后会执行异常处理程序。

#### Notification
``Notification``是一个二进制信号量集，它包含一个``Data Word``。

``seL4_Signal``()方法将所引用通知cap的标记与通知字进行``位或``(OR)，来更新通知信号标识，它还会释放等待通知的第一个线程(如果有的话)。因此，seL4_Signal()的工作方式类似于并发地发送多个信号量(由标识中设置的位表示)。如果信号发送者cap是无标记的或者说标记值是0，该操作将降级为只唤醒等待通知的第一个线程。

``seL4_Wait``()方法的工作原理类似于信号量集上select样式的等待：如果在调用seL4_Wait()时通知字为0，则调用程序将阻塞；否则，调用将立即返回，并将通知字设置为0，获得的通知字值返回给调用者。

``seL4_Poll``()与seL4_Wait()相同，只是如果没有任何信号在等待接收，调用将立即返回，而不会阻塞。

如果在调用seL4_Signal()时有线程正在等待通知信号，则第一个排队的线程将接收到通知，所有其他线程继续等待，直到下一次通知发出。

#### Fault
线程的操作可能导致错误。错误被传递给线程的错误处理程序，以便它可以采取适当的操作。错误类型在消息标签中的标号字段标识，它是以下类型之一： ``seL4_Fault_CapFault``, ``seL4_Fault_VMFault``, ``seL4_Fault_UnknownSyscall``, ``seL4_Fault_UserException``, ``seL4_Fault_DebugException``, ``seL4_Fault_TimeoutFault`` 或 ``seL4_Fault_NullFault``(表示没有发生错误，这是一条正常的IPC消息)。

错误的传递方式是模拟来自出错线程的Call调用。这意味着要发送错误消息，负责错误处理的端点能力必须具有写权限，并有``Grant``或``GrantReply``权限。如果不是这样，就会发生二次错误(通常情况下线程只是挂起)。
##### Capability Fault
cap错误可能发生在两个地方。首先，当seL4_Call()或seL4_Send()系统调用引用的cap查找失败时(对无效cap调用seL4_NBSend是静默失败)，就会发生cap错误。在这种情况下，发生错误的cap可能是正引用的 cap，也可能是在IPC缓冲区caps字段中传递的额外cap。

其次，当调用seL4_Recv()或seL4_NBRecv()时，引用不存在的cap，引用的不是端点或通知cap，或者是没有接收权限，都会导致发生cap错误。

回复错误IPC消息可以使出错线程重新启动。IPC消息内容下表给出。

| 含义                         | IPC缓冲区位置                   |
| ---------------------------- | ------------------------------- |
| 重启动地址                   | seL4_CapFault_IP                |
| cap地址                      | seL4_CapFault_Addr              |
| 是否发生在接收阶段(1是，0否) | seL4_CapFault_InRecvPhase       |
| 查找失败信息描述             | seL4_CapFault_LookupFailureType |
##### 未知系统调用
当线程使用seL4未知的``系统调用数``执行系统调用时，会发生此错误。出错线程的寄存器设置被传递给线程的错误处理程序，以便于，如在虚拟化应用场景时模拟一个系统调用。

对错误IPC的响应允许重新启动出错线程和/或修改其寄存器。如果应答的消息``标号``为0，则线程将重新启动。此外，如果消息长度非零，则会更新发生错误的线程寄存器设置。在这种情况下，更新的寄存器数量由消息标签中的length字段标识。
##### 用户异常
用户异常用于分发架构定义的异常。例如，如果用户线程试图将一个数字除以0，则可能发生这样的异常。
##### 调试
调试异常用于向线程传递跟踪和调试相关事件，如：断点、监视点、跟踪事件、指令性能采样事件，等等。内核设置了 ``CONFIG_HARDWARE_DEBUG_API`` 后就可以用上述事件支撑用户空间线程。

内核为用户空间线程提供了硬件单步执行的支持，为此引入了 ``seL4_TCB_ConfigureSingleStepping`` 系统调用。

##### Page Fault
线程可能发生页错误，响应错误IPC可以重启出错线程。IPC消息内容在下表给出。

| 含义                                      | IPC缓冲区位置              |
| ----------------------------------------- | -------------------------- |
| 重启的程序计数器                          | seL4_VMFault_IP            |
| 导致错误的地址                            | seL4_VMFault_SP            |
| 是否取指令错误(1是，0否)                  | seL4_VMFault_PrefetchFault |
| 错误状态寄存器(FSR)。依赖于架构的错误原因 | seL4_VMFault_FSR           |

#### 中断
中断作为通知信号进行传递。线程可以配置内核在每次某个中断触发时，向特定通知对象发出信号。线程可以通过调用该通知对象上的seL4_Wait()或seL4_Poll()来等待中断的发生。

IRQHandler cap表示线程配置某个中断的cap。他们有三个方法:

- ``seL4_IRQHandler_SetNotification``() 指定当中断发生时内核应该signal()的通知对象。驱动程序可以调用seL4_Wait()或seL4_Poll()在此通知对象上等待中断到来。

- ``seL4_IRQHandler_Ack``() 通知内核用户空间驱动程序已经完成了对中断的处理，然后微内核可以继续向驱动程序发送挂起的中断或新中断。

- ``seL4_IRQHandler_Clear``() 从IRQHandler中注销通知对象。

系统启动时``没有``任何IRQHandler cap，只是在系统初始线程的CSpace包含一个``IRQControl``cap。此cap能为系统中每个可用的中断生成单独的IRQHandler cap。典型地，系统初始线程确定系统中其他组件需要哪些中断，为需要的中断生成IRQHandler cap，然后将其委托给适当的驱动程序。

### Rust
#### 内存安全
大多数安全语言通过使用GC来确定何时可以安全释放内存来实现内存安全。而Rust则通过使用所有权的概念来避免运行时开销，Rust中的每个值都有一个唯一的所有者--也就是它所绑定的变量。当一个值的所有者超出范围时，这个值就会被释放。

因为只能有一个所有者，所以不允许使用别名。取而代之的是，值要么被复制(with copy trait)，要么在变量之间移动。一旦一个值被移动，它就不能再从原来的变量绑定中访问。

为了简化编程，Rust 允许对一个值的引用，称为 borrow，而不会使原始变量无效。
#### Unsafe
Rust在安全代码和unsafe代码之间有明确的区分，safe代码可以通过某些方式绕过类型系统，而unsafe代码则严格地绑定在类型系统中。具体来说，safe代码可以使用包裹在unsafe中的块来执行不安全操作(例如，解引用一个原始指针)或调用其他不安全函数。

unsafe关键字可以以两种方式使用。首先，任何代码块都可以被包裹在一个不安全块中，以允许它执行可能破坏类型系统的操作。例如，一个硬件抽象层可以使用这个特性将一个内存映射的I/O寄存器作为一个普通的Rust结构暴露出来。
```rust
let mydevice : &mut IORegs = unsafe {
  &mut *(0x200103F as *mut IORegs)
}
```
其次，函数可以被注解为unsafe，以防止不受信任的代码调用它们。例如，标准库的transmute函数可以把它的输入投射到任何其他相同大小的类型中。
```rust
pub unsafe fn transmute<T,U>(e: T) -> U
```
#### FFI(Foreign Function Interface)
FFI是外语言函数接口的简称。就像它的名字一样，FFI用于在一个程序语言的函数中调用另一个语言的函数。这一功能保障了不同程序语言之间能够自由地交互，从而可以把不同语言的代码像积木一样搭建起大型程序。

粗略地说，Rust语言提供了 ``extern`` 关键字，用来标识相关的函数能够用于同其他语言进行交互。当我们需要把Rust函数拿给C程序使用的时候，可以在函数定义前面加上extern "C"，并使用#[no_mangle]标识来保证编译器不会修改函数的名字。示例的代码如下：
```rust
fn main() {
  #[no_mangle]
  pub extern "C" fn call_rust() {
    println!("Just called a Rust function from C!");
  }
}
```
而如果要把C程序拿给Rust使用，我们需要用extern "C"标识出一个块，在块内给出C函数的声明；由于C语言对代码的限制很少，不像Rust为了保证程序的安全性而做出了很多限制，直接调用C函数可能会扰乱Rust的这些限制。因此，Rust规定，调用C函数时必须在unsafe块中进行。示例的代码如下：
```rust
extern "C" {
  fn abs(input: i32) -> i32;
}

fn main() {
  unsafe {
      println!("Absolute value of -3 according to C: {}", abs(-3));
  }
}
```
#### 条件编译
在FreeRTOS的FreeRTOSConfig.h和seL4的libsel4中，有很多用于配置的预定义。在主程序代码中，这些配置选项广泛地被用于条件编译。Rust通过 ``cfg`` 属性，也对条件编译提供了支持。

在代码中，可以采用cfg属性修饰函数：
```rust
// This function is only included when compiling for a unixish OS with a 32-bit architecture
#[cfg(all(unix, target_pointer_width = "32"))]
fn on_32bit_unix() {
  // ...
}
```

## 设计思路
#### streambuffer
//TODO
#### capability
//TODO

## 成果演示
#### streambuffer
##### 测试方法
//补充如何进行测试的以及运行结果（贴图）

## 总结
#### 项目特色
+ 在rust-freertos的基础上进行了版本更迭，新增了streambuffer模块。
+ 在rust-freertos的基础上融入一部分sel-4的特性，期望增强freertos的安全性。

#### 学习成果
+ 学习了rust语言
+ 全面学习了freertos和sel4的框架

#### 反思
+ 增强时间管理能力
+ 对可行性还要有更加深刻的论证
+ 增加组员之间以及与外部的交流

#### 展望
2021年7月8日,华为官微宣布,鸿蒙OS 2.0用户量已经突破3000万。鸿蒙生态正如火如荼，作为嵌入式实时操作系统的Freertos也或将大有可为，对Freertos的不断改进和完善也将使其得到更广泛的应用。

## 致谢
感谢邢老师为我们组选题提供了宝贵的思路和意见，感谢rust-freertos小组给我们的支持。

## 参考文献
[1]Amit Levy, Michael P Andersen, Bradford Campbell, David Culler,Prabal Dutta, Branden Ghena, Philip Levis and Pat Pannuto. Ownership is Theft: Experiences Building an Embedded OS in Rust(https://www.tockos.org/assets/papers/tock-plos2015.pdf)
[2]Is It Time to Rewrite the Operating System in Rust?(https://www.infoq.com/presentations/os-rust/)
[3]Dongkyu Jung; Daejin Park. Real Time Sensor Signal Processing Techniques Using Symmetric Dual-Bank Buffer on FreeRTOS
[4]Hsuan Hsu; Chih-Wen Hsueh. FreeRTOS Porting on x86 Platform
[5]Trustworthy Systems Team, Data61. seL4 Reference Manual(https://sel4.systems/Info/Docs/seL4-manual-latest.pdf)
[6]Tomas Docekal; Zdenek Slanina. Control system based on FreeRTOS for data acquisition and distribution on swarm robotics platform
[7]Gessé Oliveira; George Lima. Evaluation of Scheduling Algorithms for Embedded FreeRTOS-based Systems