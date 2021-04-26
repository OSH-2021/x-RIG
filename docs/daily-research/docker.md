# search_cwj

## 容器化
+ 容器软件有 LXC/LXD、Docker、Solaris Zones、Kubernetes
+ 随着容器技术的不断发展，传统容器隔离性不足的缺陷逐渐暴露了出来，为了解决传统容器隔离性不足的问题，AWS 和 Google 分别提出了 Kata Container 和 gVisor 这两种安全容器技术。
### Docker 

Docker 是 dotCloud 公司开发的一款开源的应用容器引擎，集版本控制、克隆继承、环境隔离等特性于一身，提出一整套软件构建、部署和维护的解决方案，让开发者可以打包他们的应用及依赖环境到一个可移植的容器中，然后发布到任何运行有 Docker 引擎的机器上。

Docker 提供了轻量级的虚拟化，几乎没有任何额外的开销，且容器的启动和停止都能在几秒钟内完成。与传统虚拟化技术相比具有众多优势：

- 可大大提高系统资源的利用率；
- 可在秒级内启动操作系统，而不是像传统的虚拟机技术启动应用服务需要数分钟；
- 提供了一致的运行环境；
- 现有的技术可以轻松应用；
- 允许持续交付和部署，为开发和运维人员提供极大便利；
- 由于可以确保环境的一致性，因此可以轻松地迁移；
- 由于使用分层存储和镜像技术，可以更轻松地维护和扩展。

   现有体系的弊端以及容器化技术的优势，可以作为容器化技术的可行性、有前景的参考：http://www.dockerinfo.net/494.html    

   Rancher 一种容器管理系统，是否可行？  
#### 特点

- 隔离性和安全性比较差——一旦一个容器应用出现安全问题，将影响到同一操作系统上运行的其他应用；
- 部署非常方便——使用 Docker 等容器引擎，可以实现应用的轻松分发、部署；
- 灵活性较高——容器可以迅速启动和停止，比传统的虚拟机技术快很多；
- 资源开销较小；
- 性能以传统的应用为上限——Docker 一般都要运行在精简 Linux 内核系统上，如 CoreOS、Google 的 Container optimized OS，运行速度仍然受限于 Linux 内核。
#### 安全性
为了更少的性能损失和更轻量，Docker 的隔离性和安全性显然不如传统的虚拟化技术。

我们希望的安全性一般包括：

- 容器不会对宿主机器造成影响；
- 一个容器不会对其他容器造成影响。

但是 Docker 的隔离技术其实是不完全的，存在以下问题：

- `/proc`、`/sys` 等未完全隔离；
- `top`, `free`, `iostat` 等命令展示的信息未隔离；
- `root` 用户未隔离；
- `/dev` 设备未隔离；
- 内核模块未隔离；
- SELinux、time、syslog 等所有现有 Namespace 之外的信息都未隔离

另外多个 container，system call 其实都是通过主机的内核处理，这便为Docker留下了潜在的的安全问题。

Clair：针对漏洞的漏扫工具
### Hyper
#### 介绍
Hyper采用虚拟机的方式对环境进行隔离，并不是一种基于容器的隔离方案，但它能很好地与Docker或Kubernetes等容器集群技术相结合，取代其环境隔离的功能

Hyper 是一种 App-Centric 的虚拟化技术，我们完全摒弃了传统虚机上必须和物理机一样，运行一个完整 OS 这种看似显然的假设，我们让Docker Image 直接运行在 Hypervisor 上。我们让一组容器直接启动在 hypervisor 上的时间达到 350 毫秒，并且还在进一步优化。而且所有这些，都是“开箱即得的”。

当然有人会问，有了容器为什么还要虚机。诚然，虚机并不是所有人都需要的，但是，虚机天然具备更好的隔离性；虚拟机也仍然存在于很多企业应用的协议栈中，这样一个依赖更少、开箱即得，而且还带有 Pod、persist mode 等附加丰富特性的应用，是不少场景中都需要的。而我们最期待的，就是去引爆新的容器服务 —— CaaS。

传统虚拟机的问题其实在于过于刻意模仿物理机，刻意要承载完整操作系统，启动一台虚拟机要若干秒，甚至几分钟，Image 有若干GB，加载传播都很慢，但其实根本没有这个必要，Hyper希望兼取两者的强项

对hypervisor的介绍：https://www.oschina.net/p/hyper-hypervisor?hmsr=aladdin1e1  https://my.oschina.net/u/4325071/blog/3921141
hypervisor 之于操作系统类似于操作系统之于进程。它们为执行提供独立的虚拟硬件平台，而虚拟硬件平台反过来又提供对底层机器的虚拟的完整访问。

hypervisor 分类

hypervisor 可以划分为两大类。首先是类型 1，这种 hypervisor 是直接运行在物理硬件之上的。其次是类型 2，这种 hypervisor 运行在另一个操作系统（运行在物理硬件之上）中。类型 1 hypervisor 的一个例子是基于内核的虚拟机（KVM —— 它本身是一个基于操作系统的 hypervisor）。类型 2 hypervisor 包括 QEMU 和 WINE。

### Kubernetes
Kubernetes 是一个可移植的、可扩展的开源平台，用于管理容器化的工作负载和服务，可促进声明式配置和自动化。 Kubernetes 拥有一个庞大且快速增长的生态系统。Kubernetes 的服务、支持和工具广泛可用。

名称 Kubernetes 源于希腊语，意为“舵手”或“飞行员”。Google 在 2014 年开源了 Kubernetes 项目。 Kubernetes 建立在 Google 在大规模运行生产工作负载方面拥有十几年的经验 的基础上，结合了社区中最好的想法和实践。

### KATA container
Kata Containers是一个开源项目和社区，致力于建立轻量级虚拟机（VM）的标准实现，这些虚拟机的感觉和性能类似于容器，但提供了工作负载隔离和VM的安全性优势。

了解Docker技术，就会知道，真正启动Docker容器的命令工具是RunC，它是OCI运行时规范 (runtime-spec)的默认实现。

Kata containers其实跟RunC类似，也是一个符合OCI运行时规范的一种 实现（即Clear Container和runV 都符合OCI规范），不同之处是，它给每个容器（在Docker容器的 角度）或每个Pod（k8s的角度）增加了一个独立的linux内核（不共享宿主机的内核），使容器有更好 的隔离性，安全性。

#### 现状
+ kata containers项目刚起步，还没有完整的一套组件供用户使用，只是提出了一套完整的架构。 但是从架构中看，跟clear containers目前采用的架构基本一致，包括架构图以及组件类型。因此，目 前针对kata containers的研究，可以通过clear containers入手。 

 + 目前，kata containers项目中，已经增加了包括agent，shim，proxy等组件的代码库，但是还没有针 对runtime的代码库。

+ 在后续的计划中，kata containers将分为三步来实现clear containers和runV两个项目的融合： 

+ 首先，在katacontainer中兼容clear containers 和 runV两个运行时 整体采用kata containers的架构 clear containers 和 runV两个运行时可以无缝对接kata containers的其他组件 用户可以在两个运行时之间来回切换 

+ 其次，将clear containers 和 runV合并为一个，形成kata containers自己的runtime 最后，废除clear containers 和 runV的兼容。

#### 前人开发进程
在过去的一年里 Kata Containers 支持了引入了 Firecracker 支持，将 VMM 开销降低到了 10MB 级。我们也在10月加入了 rust-agent，将 agent 的内存开销降低到了 1MB 级。我承认开销仍然存在，而且不太可能真的被压缩到0，尽管零开销隔离性是我们的理想目标。不过，目前的开发工作还在进一步降低开销。


首先，我们可以把 agent 协议的承载从 gRPC 替换成 ttRPC 的话我们可以节省很多 agent 和 shim 的内存。这个部分已经进行了一些可行性验证了，蚂蚁金服的开发者已经写了一个 ttRPC 的 Rust 实现并进行了初步测试了，尽管目前还没有具体可以发布出来的内存开销数据，但确实会节省很多，这个 ttRPC 的实现目前已经开源并正在被提交到 containerd 上游。

会上一位 Intel 的开发者提到，因为虚机自己就是个沙箱，是不是可以不需要再在沙箱内部建立一层 namespace 沙箱，也就是说，是否可以从 agent 协议的定义上彻底去掉相关操作。

Kata Containers 2.0已用Rust进行重写，结果是容器比以往更小巧更快速。据开发人员声称，这种新的Kata Containers代理其受攻击面小得多，大大提高了安全系数。然而，用户会看到从11MB缩减至300KB，大小仅为原来的十分之一。这番重写和重构还使用了ttRPC，从而进一步改善用户的资源占用空间


过去两年，Kata Containers 社区在付出一些开销的代价下，增强了容器的隔离性，同时推动了虚拟化更加的轻量化且“容器友好”。Kata Containers 项目的未来愿景是继续完善沙箱隔离，进一步降低开销，开发面向云原生的虚拟化技术，以最小的成本进一步透明地隔离云本机应用程序。Kata Containers 2.0 版本预计于今年晚些时候发布，其主要目标如下：

与已有的 Kubernetes 生态系统保持兼容；

允许将全部的应用，包括运行时进程、镜像/根文件系统等封装进沙箱中；

去掉 Agent 的非必要功能，通过重写 Rust 中的关键组件以及改进其他架构，减少对 Kata Containers 进程的封装；

改进安全性，如调整架构等，将宿主机的功能尽量留在用户空间，并让长生命周期进程可以使用非 root 权限进行；

添加对新内存缩放技术 virtio-mem 的支持，从而可在不破坏安全隔离性的情况下，可按页进行内存的扩缩容，且不再需要考虑内存条这些物理上本来并不存在的硬件的限制；

支持 cloud-hypervisor 并为 Kata Containers 的场景进行配置与定制；


#### 现状
目前，使用kata容器包装的shimv2具有一个重要的设计缺陷— IO流处理。

从结果可以明显看出，在裸机、runC容器和Kata容器之间进行选择时，需要权衡取舍。尽管runC容器为大多数用例提供了更完善的考量，但它们仍然使主机内核易于受到系统调用接口作为攻击面的利用。Kata容器提供了硬件支持的隔离，但是目前存在巨大的性能开销，尤其是对于磁盘I/O绑定操作。
(Kata1.7開始支持virtio-fs，它是一個共享的文件系統)


### Pouch
PouchContainer 是阿里巴巴集团开源的高效、轻量级企业级富容器引擎技术，拥有隔离性强、可移植性高、资源占用少等特性。可以帮助企业快速实现存量业务容器化，同时提高超大规模下数据中心的物理资源利用率。

PouchContainer 源自阿里巴巴内部场景，诞生初期，在如何为互联网应用保驾护航方面，倾尽了阿里巴巴工程师们的设计心血。PouchContainer 的强隔离、富容器等技术特性是最好的证明。在阿里巴巴的体量规模下，PouchContainer 对业务的支撑得到双 11 史无前例的检验，开源之后，阿里容器成为一项普惠技术，定位于「助力企业快速实现存量业务容器化」。

### KATA container和firecracker的I/O handling
在kata和firecracker的issue底下都有關於IO的討論

firecracker關於IO的兩個issue：
- [virtio packed queues](https://github.com/firecracker-microvm/firecracker/issues/2477)
- [virtio event notification supression](https://github.com/firecracker-microvm/firecracker/issues/2478)
我看不動了，太專業

### rust-vmm

### 参考资料
- [100个容器引擎项目，点亮你的容器集群技能树](https://zhuanlan.zhihu.com/p/36868888)
- [Hyper 基于 Hypervisor 的 Docker 引擎](https://www.oschina.net/p/hyper-hypervisor?hmsr=aladdin1e1)
- [容器漏洞评估](https://zhuanlan.zhihu.com/p/251030524)
- [Docker Container Escape](https://blog.paranoidsoftware.com/dirty-cow-cve-2016-5195-docker-container-escape/)
- [K8s文档](https://kubernetes.io/zh/docs/)
- [KATA_github](https://github.com/kata-containers/kata-containers)
- [干货｜认识kata-containers](https://blog.csdn.net/O4dC8OjO7ZL6/article/details/78986732)
- [KATA的IO性能](https://blog.csdn.net/NewTyun/article/details/106678656?ops_request_misc=%257B%2522request%255Fid%2522%253A%2522161674477216780266294794%2522%252C%2522scm%2522%253A%252220140713.130102334.pc%255Fall.%2522%257D&request_id=161674477216780266294794&biz_id=0&utm_medium=distribute.pc_search_result.none-task-blog-2~all~first_rank_v2~rank_v29-9-106678656.pc_search_result_cache&utm_term=KATA&spm=1018.2226.3001.4187)
- KATA开发者的表态
    1. [Kata Containers: 2.0的蓝图](https://mp.weixin.qq.com/s?__biz=MzUzOTk2OTQzOA==&mid=2247483919&idx=1&sn=0448ee1346cde7e9b51b3f2b9b339457&scene=21#wechat_redirect)
    2. [Kata Containers: 面向云原生的虚拟化](https://mp.weixin.qq.com/s?__biz=MzUzOTk2OTQzOA%3D%3D&idx=1&mid=2247483883&scene=21&sn=23c9ce9d31821a13bdeb2e73dc355302#wechat_redirect)
    3. [Kata Containers: 两年而立](https://mp.weixin.qq.com/s?__biz=MzUzOTk2OTQzOA==&mid=2247483874&idx=1&sn=cdc118f8c76a6bed6a6bd15153f5cb10&chksm=fac11313cdb69a055a2a200883b348a30f4d80f219b2f33a628efeccbfd6fd54efc7f7706f93&scene=21)
- [KATA更好的IO流](https://github.com/kata-containers/kata-containers/issues/151)

## FreeRTOS

初步猜测学长所说的完全实现应该是FreeRTOS是8.0版本。而现在已经到10.0

这意味着，我们是不可能把每一个port都用Rust改写一遍的。但是，所有的port函数都提供了统一的API接口，所以我们决定利用Rust封装这些API接口。有了这些封装，**我们的代码理论上可以在任何FreeRTOS和LLVM支持的平台上运行**。

2018.8.31 10.1
2019.5.18 10.2
2020.2.13 10.3


### Reference
[FreeRTOS代码级介绍](https://blog.csdn.net/qq_37634122/article/details/104302394?spm=1001.2014.3001.5501)