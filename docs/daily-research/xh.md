# Weensy OS

    关于os/161的介绍文档以及对cs 161 assignment的介绍（2016年版本）：http://www.eecs.harvard.edu/~dholland/pubs/sigcse-02/sigcse-02.pdf

1.  Weensy OS这个概念来自于CS61 problem set 3 链接：http://cs61.seas.harvard.edu/site/2020/CourseDescription/

    1.  对cs 61的介绍
    cs 61：Systems Programming and Machine Organization (2020)
    CS 61 is an introduction to the fundamentals of computer systems programming. Topics include C, C++, and assembly language programming, performance analysis and improvement strategies, memory management, caching, concurrency, threads, and synchronization.

    CS 61 will help you develop the skills to write programs for the real world, where performance and robustness really matter. It will also prepare you for more advanced CS courses, including operating systems, compilers and programming languages, architecture, and graphics. We want it to be fun and challenging.

    2.  对设备的要求
    This course assumes access to an x86-64-based Linux machine. All problem sets and lecture code can run on a Linux host. Most problem sets and lecture code can also run directly on a Mac OS X host or on Windows Subsystem for Linux. However, for problem sets, we recommend you at least check your work on Ubuntu or another Linux host; your work should not accidentally rely on Mac-specific or Windows-specific behavior.

    If you don’t already have a Linux machine, there are two main ways to get one: either via Docker or via a virtual machine. Docker support is new this year and may be preferred for many students. We recommend you try Docker first.
    http://cs61.seas.harvard.edu/site/2020/Infrastructure/

2.  对WeensyOS的介绍：http://cs61.seas.harvard.edu/site/2020/WeensyOS/
    In this assignment, you implement process memory isolation, virtual memory, and some system calls in a tiny operating system. This will introduce you to virtual memory and operating system design.

    weensyos的分步任务：
    1.  Kernel isolation
    2.  Isolated address spaces
    3.  Virtual page allocation
    4.  Overlapping address spaces
    5.  Fork
    6.  Freeing memory
    7.  Shared read-only memory

3.  CS 161: Operating Systems (2021)    https://read.seas.harvard.edu/cs161/2021/
    1.  cs 161 概览：
    This is an in-depth course in operating systems design and implementation, focusing on multicore operating systems kernels.
    The course uses Chickadee, an operating system based on CS 61 WeensyOS. Chickadee takes advantage of newer hardware, language, and OS design features than many teaching operating systems.

    注意：Chickadee, an operating system based on CS 61 WeensyOS.Chickadee takes advantage of newer hardware, language, and OS design features than many teaching operating systems.使用的操作系统是Chickadee。https://github.com/cs161/chickadee/

    2.  对环境的要求：Linux instructions
    The native Chickadee build environment is Linux. To use Linux, either set up your own Linux machine or prepare and install a Linux virtual machine on your preferred computer.

    3.  Chickadee 概览（多处理器操作系统）
    Chickadee, unlike WeensyOS, is a multiprocessor operating system; and Chickadee, unlike WeensyOS, supports kernel task suspension: a Chickadee kernel processing a system call on behalf of one process can suspend itself, either voluntarily or because of an interrupt, and allow other tasks to run, resuming later in kernel space exactly where it left off. Kernel task suspension has profound effects on operating system structure.
    github有关代码：https://github.com/cs161/chickadee/

    4.  cs 161 任务：不断完善chicadee，优化各项功能 (included in the problem sets)
        Problem set 1: buddy allocation
        Problem set 2: Process hierarchies and wait queues
        Problem set 3: File descriptors and VFS
        Problem set 4: Disk and buffer cache

4.  关于cs 161的多个课程网址：
    1.  http://os161.eecs.harvard.edu/  （2016年）
        含有较为详细的说明，关于操作系统已有的框架以及需要学生完成的部分。这里指的操作系统被称为OS/161，但可能就是上文提到的chickadee？
        这里提供了关于这个操作系统的详细的介绍：http://www.eecs.harvard.edu/~dholland/pubs/sigcse-02/sigcse-02.pdf 介绍了设计新的教学操作系统的原因，以及对现有的其他的教学型操作系统的介绍。称之为教学型操作系统，因为它结构更加简单，但又反映了真正OS的设计和层次，需要学生进行完善

        可以仔细地看一下这个文档，详细地介绍了这个OS/161的相关信息，以及cs 161的计划http://www.eecs.harvard.edu/~dholland/pubs/sigcse-02/sigcse-02.pdf（已置顶）

        关于conclusion
        In the introduction, we listed six design goals forOS/161 and System/161. In our estimation, with onexception, we have accomplished them. We created a realistic execution environment that supported easy debugging; we wrote an operating system that remains sufficiently simple for instructional use, but reflects the design and layout of real OSes; and we believe that the
        code we have is cleanly written and well organized. We did have several students go out of their way to tell us that they liked the code.

        Unfortunately, it is not yet the case that the finished OS/161 properly supports “real” user applications. This is, however, largely because OS/161’s C library is not yet sufficiently complete; this problem is easily solved, and we hope to port over some of BSD /usr/games before long.

        We believe we have produced a useful system and instructional tool and hope others may find it useful for themselves as well.

        关于Availability
        OS/161 and System/161 are freely redistributable under a BSD-like license and are available for download from ftp://ftp.eecs.harvard.edu/pub/os161.


    2.  http://www.eecs.harvard.edu/~cs161/syllabus.html      （2017年）
        2016年和2017年的课程包含了教学计划以及对应的教学内容（教学大纲）和PPT,可以参考 http://www.eecs.harvard.edu/~cs161/syllabus.html

    3.  https://www.read.seas.harvard.edu/cs161/2018/     （2018年）
        从2018年开始，课程使用了新的页面，以及开始使用chickadee来称呼新的操作系统（基于weensy os）