# seL4 Research
## 1. seL4, C'est Quoi ?
經過形式化驗證的微內核與Hypervisor(also named Virtual Machine Monitor)，很快很安全

seL4 should be pronounced as 's-e-l-4' not 'sel-4'

seL4是一种高安全性，高性能的操作系统微内核。它是唯一的，因为它进行了全面的正式验证，而不会影响性能。它旨在用作构建安全性和安全性至关重要的系统的可信赖基础。

作为内核意味着它是在任何软件系统的核心运行并控制对资源的所有访问的软件。它通过功能提供细粒度的访问控制，并控制系统组件之间的通信。它是软件系统中最关键的部分，并以特权模式运行。

seL4是L4微内核家族的成员，并且是世界上最先进，最有保证的操作系统内核。

seL4的正式验证使它与任何其他操作系统都脱颖而出。简而言之，它为系统中运行的应用程序之间的隔离提供了最高的保证，这意味着可以遏制系统某一部分的妥协，并防止损害系统中其他可能更关键的部分。

具体来说，seL4的实现已在形式上（数学上）相对于其规范得到了正确的证明（无错误），并被证明具有强大的安全性，并且，如果配置正确，其操作在最坏情况下的执行时间也已被证明是安全的上限。它是世界上第一个具有这种证明的操作系统，并且仍然是唯一经过验证的，具有基于细粒度功能的安全性和高性能的操作系统。它还对混合关键性实时系统提供了最先进的支持。

## 2. 微內核(Microkernel)與宏內核(Monolithic kernel)
   ![comparison](https://s3.us-west-2.amazonaws.com/secure.notion-static.com/e4165605-9040-4ded-b9d0-352f91a1c23e/Untitled.png?X-Amz-Algorithm=AWS4-HMAC-SHA256&X-Amz-Credential=AKIAT73L2G45O3KS52Y5%2F20210324%2Fus-west-2%2Fs3%2Faws4_request&X-Amz-Date=20210324T015609Z&X-Amz-Expires=86400&X-Amz-Signature=10edcb189addd31e4256f8fa46f2adfca328c0772ce0b0288a6c744aa24a8058&X-Amz-SignedHeaders=host&response-content-disposition=filename%20%3D%22Untitled.png%22)

| aaa                          | monolithic      | micro        |
| ---------------------------- | --------------- | ------------ |
| access mode                  | privileged mode | user mode    |
| trusted computing base (TCB) | BIG(20m)        | small(10k)   |
| services                     | in the kernel   | outside, IPC |

## 3. History
略

## 4. seL4 Features
1. CAPABILITY

    Capability是用于提供访问系统中对象权限的凭证。seL4系统中所有资源的capability在启动时都被授予根进程。要对任何资源进行操作，用户都需使用libsel4中的内核API，并提供相应的capability。

    ### kernel objects:
    - ``Endpoints`` are used to perform protected function calls;
    - ``Reply Objects`` represent a return path from a protected procedure(functions) call;
    - ``Address Spaces`` provide the sandboxes around components (thin wrappers abstracting hardware page tables);
    - ``Cnodes`` store capabilities representing a component’s access rights;
    - ``Thread Control Blocks`` represent threads of execution;
    - ``Scheduling Contexts`` represent the right to access a certain fraction of execution time on a core;
    - ``Notifications`` are synchronisation objects (similar to semaphores);
    - ``Frames`` represent physical memory that can be mapped into address spaces (pages)
    - ``Interrupt Objects`` provide access to interrupt handling;
    - ``Untypeds`` unused (free) physical memory that can be converted (“retyped”) intoany of the other types.(調用seL4_Untyped_Retype method)

2. Resource-Management Policy at User Level(painful to use, L4-like programming model)
3. F.V.
4. No Abstraction没有抽象(not minimal)
5. IPC : handshake between endpoints (no buffer)
6. seL4对使用C进行了如下限制：
   1. 栈变量不得取引用，必要时以全局变量代替
   2. 禁止函数指针
   3. 不支持union

## 5. Previous Work
- [x-qwq](https://github.com/OSH-2019/x-qwq): 他們的工作主要是改寫了內核(part of capability, stack, thread)以及kernel object(TCB 和 untyped)。大約五千行代碼
- [Redox](https://github.com/redox-os/redox): 完全使用Rust編寫的操作系統，也是微內核設計，而且是一個``full-featured Operating System``, providing packages (memory allocator, file system, display manager, core utilities, etc.) that together make up a functional and convenient operating system.(seL4沒有這些東西)

## 6. What can we do?
- 將seL4剩下的東西(libsel4, syscall etc.)全部用rust重寫
- Build an 'ecosystem' for ``x-qwq``, could follow the guidelines of the [course project](https://www.cse.unsw.edu.au/~cs9242/20/project/index.shtml)
- Get seL4 running on Raspberry PI
- 把微內核變得和宏內核一樣快(

****
## some ideas

- [ ] 有没有可能将sel4的capability移植到FreeRTOS上

[Chapter5](https://sel4.systems/About/seL4-whitepaper.pdf)

通过capability实现对处理器使用效率的优化，而且还有各种成分之间的隔离

sel4's capability
-  object-oriented对象优先
-  fine-grained
-  系统干预access
-  授权模式，用户之间的授权可随时通过摧毁capability以取消授权


Linux's access control
-  subject-oriented主体优先(rwx)
-  coarse-grained
-  confused deputy

例子

```
ACL based system

Alice$ gcc -o prog.o prog.c

gcc will generate a log

Alice --> gcc --> log file(system privilege)

Alice specifys the output file ---- passwd

Alice --X-> passwd

Alice --> gcc --> passwd

OS checks gcc's subject ID and it's OK for gcc to write to passwd

but Alice has no access to passwd herself

-------------------------------------------------------------
sel4's capability passes Alice's capability to gcc and gcc uses Alice's capability to open a new file which Alice has access

so Alice can't modify the passwd with the capability mechanism

gcc itself can open the log file because it's the operation's launcher and it has the capability to open the log file.
```

如果真要说sel4的capability与linux的capability有什么区别的话，就是sel4的capability是init程序开始的时候就有的，而linux的capability是你自己可选的

## Refs
[sel4 Tutorials for different goals](https://docs.sel4.systems/Tutorials/)

> 里面有讲到port sel4 to a new platform(可能是移植)，还有是开发sel4-based框架以及系统

[seL4 Official Site](https://sel4.systems/)

[seL4 Whitepaper](https://sel4.systems/About/seL4-whitepaper.pdf)(an introduction to seL4)

UNSW基于seL4的[Advanced Operating system](http://www.cse.unsw.edu.au/~cs9242/current/) 有[錄像](https://www.youtube.com/playlist?list=PLbSaCpDlfd6qLbEsKquVo3--0gwYBmrUV), [B站搬運](https://space.bilibili.com/1627303/video) |
the course uses odroid-c2 (a board like raspberry pi)

[seL4 on the Raspberry Pi 3](https://research.csiro.au/tsblog/sel4-raspberry-pi-3/)

[seL4 lateset manual](https://sel4.systems/Info/Docs/seL4-manual-latest.pdf)

> 注意，在这个manual中提到这是从用户角度看kernel，要真正了解细节，还需要看abstract specification

[seL4 capability PPT](cl.cam.ac.uk/research/security/ctsrd/cheri/workshops/pdfs/20160423-sel4-capabilities.pdf)

