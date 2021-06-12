/*
 * Copyright 2014, General Dynamics C4 Systems
 *
 * SPDX-License-Identifier: GPL-2.0-only
 */

#include <assert.h>
#include <kernel/boot.h>
#include <kernel/thread.h>
#include <machine/io.h>
#include <machine/registerset.h>
#include <model/statedata.h>
#include <arch/machine.h>
#include <arch/kernel/boot.h>
#include <arch/kernel/vspace.h>
#include <linker.h>
#include <hardware.h>
#include <util.h>
/*Z 引导时用的全局变量，包括内存、bootinfo、CSlot索引等。隐式地初始化为全0 */
/* (node-local) state accessed only during bootstrapping */
ndks_boot_t ndks_boot BOOT_DATA;

rootserver_mem_t rootserver BOOT_DATA;  /*Z 相当于Linux的init进程的有关重要信息 */
static region_t rootserver_mem BOOT_DATA;   /*Z 临时变量，记录还需要为initrd线程分配的内存 */
/*Z 将ndks_boot中的保留内存区域列表中地址相邻的区域合并 */
BOOT_CODE static void merge_regions(void)
{
    /* Walk through reserved regions and see if any can be merged */
    for (word_t i = 1; i < ndks_boot.resv_count;) {
        if (ndks_boot.reserved[i - 1].end == ndks_boot.reserved[i].start) {
            /* extend earlier region */
            ndks_boot.reserved[i - 1].end = ndks_boot.reserved[i].end;
            /* move everything else down */
            for (word_t j = i + 1; j < ndks_boot.resv_count; j++) {
                ndks_boot.reserved[j - 1] = ndks_boot.reserved[j];
            }

            ndks_boot.resv_count--;
            /* don't increment i in case there are multiple adjacent regions */
        } else {
            i++;
        }
    }
}
/*Z 将内存区域加入到ndks_boot的保留区域列表中，返回值1成功，0失败 */
BOOT_CODE bool_t reserve_region(p_region_t reg)
{
    word_t i;
    assert(reg.start <= reg.end);
    if (reg.start == reg.end) {
        return true;
    }

    /* keep the regions in order */
    for (i = 0; i < ndks_boot.resv_count; i++) {
        /* Try and merge the region to an existing one, if possible */
        if (ndks_boot.reserved[i].start == reg.end) {
            ndks_boot.reserved[i].start = reg.start;
            merge_regions();
            return true;
        }
        if (ndks_boot.reserved[i].end == reg.start) {
            ndks_boot.reserved[i].end = reg.end;
            merge_regions();
            return true;
        }
        /* Otherwise figure out where it should go. */
        if (ndks_boot.reserved[i].start > reg.end) {
            /* move regions down, making sure there's enough room */
            if (ndks_boot.resv_count + 1 >= MAX_NUM_RESV_REG) {
                printf("Can't mark region 0x%lx-0x%lx as reserved, try increasing MAX_NUM_RESV_REG (currently %d)\n",
                       reg.start, reg.end, (int)MAX_NUM_RESV_REG);
                return false;
            }
            for (word_t j = ndks_boot.resv_count; j > i; j--) {
                ndks_boot.reserved[j] = ndks_boot.reserved[j - 1];
            }
            /* insert the new region */
            ndks_boot.reserved[i] = reg;
            ndks_boot.resv_count++;
            return true;
        }
    }

    if (i + 1 == MAX_NUM_RESV_REG) {
        printf("Can't mark region 0x%lx-0x%lx as reserved, try increasing MAX_NUM_RESV_REG (currently %d)\n",
               reg.start, reg.end, (int)MAX_NUM_RESV_REG);
        return false;
    }

    ndks_boot.reserved[i] = reg;
    ndks_boot.resv_count++;

    return true;
}
/*Z 将线性地址区域加入到ndks_boot的保留和free区域列表 */
BOOT_CODE bool_t insert_region(region_t reg)
{
    word_t i;

    assert(reg.start <= reg.end);
    if (is_reg_empty(reg)) {
        return true;
    }
    for (i = 0; i < MAX_NUM_FREEMEM_REG; i++) {
        if (is_reg_empty(ndks_boot.freemem[i])) {
            reserve_region(pptr_to_paddr_reg(reg));
            ndks_boot.freemem[i] = reg;
            return true;
        }
    }
#ifdef CONFIG_ARCH_ARM
    /* boot.h should have calculated MAX_NUM_FREEMEM_REG correctly.
     * If we've run out, then something is wrong.
     * Note that the capDL allocation toolchain does not know about
     * MAX_NUM_FREEMEM_REG, so throwing away regions may prevent
     * capDL applications from being loaded! */
    printf("Can't fit memory region 0x%lx-0x%lx, try increasing MAX_NUM_FREEMEM_REG (currently %d)\n",
           reg.start, reg.end, (int)MAX_NUM_FREEMEM_REG);
    assert(!"Ran out of freemem slots");
#else
    printf("Dropping memory region 0x%lx-0x%lx, try increasing MAX_NUM_FREEMEM_REG (currently %d)\n",
           reg.start, reg.end, (int)MAX_NUM_FREEMEM_REG);
#endif
    return false;
}
/*Z 为initrd分配n*(2^size_bits)大小的内存，清零并返回开始线性地址 */
BOOT_CODE static pptr_t alloc_rootserver_obj(word_t size_bits, word_t n)
{
    pptr_t allocated = rootserver_mem.start;
    /* allocated memory must be aligned */
    assert(allocated % BIT(size_bits) == 0);
    rootserver_mem.start += (n * BIT(size_bits));
    /* we must not have run out of memory */
    assert(rootserver_mem.start <= rootserver_mem.end);
    memzero((void *) allocated, n * BIT(size_bits));
    return allocated;
}
/*Z 返回参数、CNode、VSpace中的最大值 */
BOOT_CODE static word_t rootserver_max_size_bits(word_t extra_bi_size_bits)
{
    word_t cnode_size_bits = CONFIG_ROOT_CNODE_SIZE_BITS + seL4_SlotBits;
    word_t max = MAX(cnode_size_bits, seL4_VSpaceBits);
    return MAX(max, extra_bi_size_bits);
}
/*Z 计算rootserver(initrd)还需要分配的内存，包括：IPC buffer、bootinfo、TCB、CNode、页表等 */
BOOT_CODE static word_t calculate_rootserver_size(v_region_t v_reg, word_t extra_bi_size_bits)
{
    /* work out how much memory we need for root server objects */
    word_t size = BIT(CONFIG_ROOT_CNODE_SIZE_BITS + seL4_SlotBits); /*Z 根CNode有8192个CSlot，每个CSlot大小32字节 */
    size += BIT(seL4_TCBBits); // root thread tcb                   /*Z 线程控制块(TCB)2K */
    size += 2 * BIT(seL4_PageBits); // boot info + ipc buf          /*Z IPC buffer和bootinfo共8K */
    size += BIT(seL4_ASIDPoolBits);                                 /*Z ASID(PCID) pool 4K */
    size += extra_bi_size_bits > 0 ? BIT(extra_bi_size_bits) : 0;
    size += BIT(seL4_VSpaceBits); // root vspace                    /*Z VSpace 4K */
#ifdef CONFIG_KERNEL_MCS
    size += BIT(seL4_MinSchedContextBits); // root sched context    /*Z 最小的调度上下文(对数) */
#endif  /*Z 区域需要的页表页总数（除一级页表页），包括老旧设备DMA保留区占用的IOMMU页表 */
    /* for all archs, seL4_PageTable Bits is the size of all non top-level paging structures */
    return size + arch_get_n_paging(v_reg) * BIT(seL4_PageTableBits);
}
/*Z 如果额外bootinfo尺寸大，就分配其内存，否则返回 */
BOOT_CODE static void maybe_alloc_extra_bi(word_t cmp_size_bits, word_t extra_bi_size_bits)
{
    if (extra_bi_size_bits >= cmp_size_bits && rootserver.extra_bi == 0) {
        rootserver.extra_bi = alloc_rootserver_obj(extra_bi_size_bits, 1);
    }
}
/*Z 从线性地址start处开始，为initrd线程分配rootserver数据结构内存。不好：没有为可能的IOMMU老旧DMA保留内存分配页表 */
BOOT_CODE void create_rootserver_objects(pptr_t start, v_region_t v_reg, word_t extra_bi_size_bits)
{
    /* the largest object the PD, the root cnode, or the extra boot info */
    word_t cnode_size_bits = CONFIG_ROOT_CNODE_SIZE_BITS + seL4_SlotBits;
    word_t max = rootserver_max_size_bits(extra_bi_size_bits);
    /*Z 不好：这代码重复的这么多 */
    word_t size = calculate_rootserver_size(v_reg, extra_bi_size_bits);
    rootserver_mem.start = start;
    rootserver_mem.end = start + size;/*Z 这些是所有需要分配的内存大小 */
    /*Z 从大到小谨尊对齐分配内存 */
    maybe_alloc_extra_bi(max, extra_bi_size_bits);

    /* the root cnode is at least 4k, so it could be larger or smaller than a pd. */
#if (CONFIG_ROOT_CNODE_SIZE_BITS + seL4_SlotBits) > seL4_VSpaceBits
    rootserver.cnode = alloc_rootserver_obj(cnode_size_bits, 1);/*Z 分配CNode内存 */
    maybe_alloc_extra_bi(seL4_VSpaceBits, extra_bi_size_bits);/*Z 可能分配额外bootinfo内存 */
    rootserver.vspace = alloc_rootserver_obj(seL4_VSpaceBits, 1);/*Z 分配VSpace内存 */
#else
    rootserver.vspace = alloc_rootserver_obj(seL4_VSpaceBits, 1);
    maybe_alloc_extra_bi(cnode_size_bits, extra_bi_size_bits);
    rootserver.cnode = alloc_rootserver_obj(cnode_size_bits, 1);
#endif

    /* at this point we are up to creating 4k objects - which is the min size of
     * extra_bi so this is the last chance to allocate it */
    maybe_alloc_extra_bi(seL4_PageBits, extra_bi_size_bits);
    rootserver.asid_pool = alloc_rootserver_obj(seL4_ASIDPoolBits, 1);
    rootserver.ipc_buf = alloc_rootserver_obj(seL4_PageBits, 1);
    rootserver.boot_info = alloc_rootserver_obj(seL4_PageBits, 1);

    /* TCBs on aarch32 can be larger than page tables in certain configs */
#if seL4_TCBBits >= seL4_PageTableBits
    rootserver.tcb = alloc_rootserver_obj(seL4_TCBBits, 1);
#endif

    /* paging structures are 4k on every arch except aarch32 (1k) */
    word_t n = arch_get_n_paging(v_reg);/*Z 分配页表（除一级）内存 */
    rootserver.paging.start = alloc_rootserver_obj(seL4_PageTableBits, n);
    rootserver.paging.end = rootserver.paging.start + n * BIT(seL4_PageTableBits);

    /* for most archs, TCBs are smaller than page tables */
#if seL4_TCBBits < seL4_PageTableBits
    rootserver.tcb = alloc_rootserver_obj(seL4_TCBBits, 1);
#endif

#ifdef CONFIG_KERNEL_MCS
    rootserver.sc = alloc_rootserver_obj(seL4_MinSchedContextBits, 1);
#endif
    /* we should have allocated all our memory */
    assert(rootserver_mem.start == rootserver_mem.end);
}
/*Z 将能力写入指定的CSlot */
BOOT_CODE void write_slot(slot_ptr_t slot_ptr, cap_t cap)
{
    slot_ptr->cap = cap;

    slot_ptr->cteMDBNode = nullMDBNode;
    mdb_node_ptr_set_mdbRevocable(&slot_ptr->cteMDBNode, true);
    mdb_node_ptr_set_mdbFirstBadged(&slot_ptr->cteMDBNode, true);
}

/* Our root CNode needs to be able to fit all the initial caps and not
 * cover all of memory.
 *//*Z 断言CNode总大小<4G且>=4K，CSlot数量至少满足初始数量 */
compile_assert(root_cnode_size_valid,
               CONFIG_ROOT_CNODE_SIZE_BITS < 32 - seL4_SlotBits &&
               BIT(CONFIG_ROOT_CNODE_SIZE_BITS) >= seL4_NumInitialCaps &&
               BIT(CONFIG_ROOT_CNODE_SIZE_BITS) >= (seL4_PageBits - seL4_SlotBits))
/*Z 初始化rootserver的CNode，创建一个指向自身且能操作本身的能力CSlot */
BOOT_CODE cap_t
create_root_cnode(void)
{
    /* write the number of root CNode slots to global state */
    ndks_boot.slot_pos_max = BIT(CONFIG_ROOT_CNODE_SIZE_BITS);
    /*Z 生成以rootserver.cnode值为CNode地址，能操作该CNode的能力 */
    cap_t cap =
        cap_cnode_cap_new(
            CONFIG_ROOT_CNODE_SIZE_BITS,      /* radix      *//*Z CSlot索引位位数 */
            wordBits - CONFIG_ROOT_CNODE_SIZE_BITS, /* guard size *//*Z 保护位位数。保护位和索引位用完了64位，意味着不能以此CNode为根，寻址别的CNode中的能力 */
            0,                                /* guard      *//*Z 保护位模式 */
            rootserver.cnode              /* pptr       */
        );

    /* write the root CNode cap into the root CNode */
    write_slot(SLOT_PTR(rootserver.cnode, seL4_CapInitThreadCNode), cap);/*Z 在相应数组项中写入能力 */

    return cap;
}

/* Check domain scheduler assumptions. */
compile_assert(num_domains_valid,
               CONFIG_NUM_DOMAINS >= 1 && CONFIG_NUM_DOMAINS <= 256)
compile_assert(num_priorities_valid,
               CONFIG_NUM_PRIORITIES >= 1 && CONFIG_NUM_PRIORITIES <= 256)
/*Z 为能力所指CNode创建调度域控制能力。未实现 */
BOOT_CODE void
create_domain_cap(cap_t root_cnode_cap)
{
    /* Check domain scheduler assumptions. */
    assert(ksDomScheduleLength > 0);
    for (word_t i = 0; i < ksDomScheduleLength; i++) {
        assert(ksDomSchedule[i].domain < CONFIG_NUM_DOMAINS);
        assert(ksDomSchedule[i].length > 0);
    }

    cap_t cap = cap_domain_cap_new();
    write_slot(SLOT_PTR(pptr_of_cap(root_cnode_cap), seL4_CapDomain), cap);
}

/*Z 为IPC buffer在CNode中创建物理页映射能力，在VSpace中创建页表项(映射到vptr) */
BOOT_CODE cap_t create_ipcbuf_frame_cap(cap_t root_cnode_cap, cap_t pd_cap, vptr_t vptr)
{
    clearMemory((void *)rootserver.ipc_buf, PAGE_BITS);

    /* create a cap of it and write it into the root CNode */
    cap_t cap = create_mapped_it_frame_cap(pd_cap, rootserver.ipc_buf, vptr, IT_ASID, false, false);
    write_slot(SLOT_PTR(pptr_of_cap(root_cnode_cap), seL4_CapInitThreadIPCBuffer), cap);

    return cap;
}
/*Z 为bootinfo在CNode中创建物理页映射能力，在VSpace中创建页表项(映射到vptr) */
BOOT_CODE void create_bi_frame_cap(cap_t root_cnode_cap, cap_t pd_cap, vptr_t vptr)
{
    /* create a cap of it and write it into the root CNode *//*Z 为物理页创建映射能力，建立普通权限的页表项 */
    cap_t cap = create_mapped_it_frame_cap(pd_cap, rootserver.boot_info, vptr, IT_ASID, false, false);
    write_slot(SLOT_PTR(pptr_of_cap(root_cnode_cap), seL4_CapBootInfoFrame), cap);
}
/*Z 参数按2^n页大小向上取整后值的位数：4K-12, 2×4K-13, 3×4K-14 */
BOOT_CODE word_t calculate_extra_bi_size_bits(word_t extra_size)
{
    if (extra_size == 0) {
        return 0;
    }

    word_t clzl_ret = clzl(ROUND_UP(extra_size, seL4_PageBits));
    word_t msb = seL4_WordBits - 1 - clzl_ret;
    /* If region is bigger than a page, make sure we overallocate rather than underallocate */
    if (extra_size > BIT(msb)) {
        msb++;
    }
    return msb;
}
/*Z 赋值initrd(rootserver)的部分bootinfo */
BOOT_CODE void populate_bi_frame(node_id_t node_id, word_t num_nodes, vptr_t ipcbuf_vptr,
                                 word_t extra_bi_size)
{
    clearMemory((void *) rootserver.boot_info, BI_FRAME_SIZE_BITS);
    if (extra_bi_size) {
        clearMemory((void *) rootserver.extra_bi, calculate_extra_bi_size_bits(extra_bi_size));
    }

    /* initialise bootinfo-related global state */
    ndks_boot.bi_frame = BI_PTR(rootserver.boot_info);
    ndks_boot.slot_pos_cur = seL4_NumInitialCaps;
    BI_PTR(rootserver.boot_info)->nodeID = node_id;
    BI_PTR(rootserver.boot_info)->numNodes = num_nodes;
    BI_PTR(rootserver.boot_info)->numIOPTLevels = 0;/*Z 初始化IOMMU页表级数为0 */
    BI_PTR(rootserver.boot_info)->ipcBuffer = (seL4_IPCBuffer *) ipcbuf_vptr;
    BI_PTR(rootserver.boot_info)->initThreadCNodeSizeBits = CONFIG_ROOT_CNODE_SIZE_BITS;
    BI_PTR(rootserver.boot_info)->initThreadDomain = ksDomSchedule[ksDomScheduleIdx].domain;/*Z 这里用到的ksDomScheduleIdx是初始的0值 */
    BI_PTR(rootserver.boot_info)->extraLen = extra_bi_size;
}
/*Z 在rootserver的根CNode当前空闲CSlot写入能力，并递进空闲CSlot指针 */
BOOT_CODE bool_t provide_cap(cap_t root_cnode_cap, cap_t cap)
{
    if (ndks_boot.slot_pos_cur >= ndks_boot.slot_pos_max) {
        printf("Kernel init failed: ran out of cap slots\n");
        return false;
    }
    write_slot(SLOT_PTR(pptr_of_cap(root_cnode_cap), ndks_boot.slot_pos_cur), cap);
    ndks_boot.slot_pos_cur++;
    return true;
}
/*Z 为线性区域在CNode中创建物理页映射能力，视情在VSpace中创建页表项 */
BOOT_CODE create_frames_of_region_ret_t create_frames_of_region(
    cap_t    root_cnode_cap,    /*Z CNode */
    cap_t    pd_cap,            /*Z VSpace */
    region_t reg,               /*Z 线性区域 */
    bool_t   do_map,            /*Z 是否作映射 */
    sword_t  pv_offset          /*Z 物理地址到虚拟地址的差值 */
)
{
    pptr_t     f;
    cap_t      frame_cap;
    seL4_SlotPos slot_pos_before;
    seL4_SlotPos slot_pos_after;

    slot_pos_before = ndks_boot.slot_pos_cur;

    for (f = reg.start; f < reg.end; f += BIT(PAGE_BITS)) {
        if (do_map) {   /*Z 为物理页创建映射能力，建立普通权限的页表项 *//*Z 不好：pptr_to_paddr应该只括第1个值，最终结果是虚拟地址 */
            frame_cap = create_mapped_it_frame_cap(pd_cap, f, pptr_to_paddr((void *)(f - pv_offset)), IT_ASID, false, true);
        } else {        /*Z 为物理页创建未映射的能力 */
            frame_cap = create_unmapped_it_frame_cap(f, false);
        }/*Z 在rootserver的根CNode当前空闲CSlot写入能力，并递进空闲CSlot指针 */
        if (!provide_cap(root_cnode_cap, frame_cap))
            return (create_frames_of_region_ret_t) {
            S_REG_EMPTY, false
        };
    }

    slot_pos_after = ndks_boot.slot_pos_cur;

    return (create_frames_of_region_ret_t) {
        (seL4_SlotRegion) { slot_pos_before, slot_pos_after }, true
    };
}
/*Z 为initrd创建ASID(PCID) pool能力 */
BOOT_CODE cap_t create_it_asid_pool(cap_t root_cnode_cap)
{
    cap_t ap_cap = cap_asid_pool_cap_new(IT_ASID >> asidLowBits, rootserver.asid_pool);
    write_slot(SLOT_PTR(pptr_of_cap(root_cnode_cap), seL4_CapInitThreadASIDPool), ap_cap);

    /* create ASID control cap */
    write_slot(
        SLOT_PTR(pptr_of_cap(root_cnode_cap), seL4_CapASIDControl),
        cap_asid_control_cap_new()
    );

    return ap_cap;
}

#ifdef CONFIG_KERNEL_MCS
/*Z 配置TCB的SC，设置refill有关数据。注意：这里设置调度周期为0，即采用的是RoundRobin算法，而不是严格的周期限制 */
BOOT_CODE static bool_t configure_sched_context(tcb_t *tcb, sched_context_t *sc_pptr, ticks_t timeslice, word_t core)
{
    tcb->tcbSchedContext = sc_pptr;
    REFILL_NEW(tcb->tcbSchedContext, MIN_REFILLS, timeslice, 0, core);

    tcb->tcbSchedContext->scTcb = tcb;
    return true;
}
/*Z 赋予CNode对每个cpu的SchedControl能力 */
BOOT_CODE bool_t init_sched_control(cap_t root_cnode_cap, word_t num_nodes)
{
    bool_t ret = true;
    seL4_SlotPos slot_pos_before = ndks_boot.slot_pos_cur;
    /* create a sched control cap for each core */
    for (int i = 0; i < num_nodes && ret; i++) {
        ret = provide_cap(root_cnode_cap, cap_sched_control_cap_new(i));
    }

    if (!ret) {
        return false;
    }

    /* update boot info with slot region for sched control caps */
    ndks_boot.bi_frame->schedcontrol = (seL4_SlotRegion) {
        .start = slot_pos_before,
        .end = ndks_boot.slot_pos_cur
    };

    return true;
}
#endif
/*Z 创建idle线程，配置相关的TCB和SC数据 */
BOOT_CODE bool_t create_idle_thread(void)
{
    pptr_t pptr;

#ifdef ENABLE_SMP_SUPPORT
    for (int i = 0; i < CONFIG_MAX_NUM_NODES; i++) {
#endif /* ENABLE_SMP_SUPPORT */
        pptr = (pptr_t) &ksIdleThreadTCB[SMP_TERNARY(i, 0)];/*Z 获取TCB地址 */
        NODE_STATE_ON_CORE(ksIdleThread, i) = TCB_PTR(pptr + TCB_OFFSET);/*Z TCB的1K处是tcb_t */
        configureIdleThread(NODE_STATE_ON_CORE(ksIdleThread, i));/*Z 设置i预先保存的上下文寄存器和线程状态 */
#ifdef CONFIG_DEBUG_BUILD
        setThreadName(NODE_STATE_ON_CORE(ksIdleThread, i), "idle_thread");/*Z 设置线程名 */
#endif
        SMP_COND_STATEMENT(NODE_STATE_ON_CORE(ksIdleThread, i)->tcbAffinity = i);
#ifdef CONFIG_KERNEL_MCS
        bool_t result = configure_sched_context(NODE_STATE_ON_CORE(ksIdleThread, i), SC_PTR(&ksIdleThreadSC[SMP_TERNARY(i, 0)]),
                                                usToTicks(CONFIG_BOOT_THREAD_TIME_SLICE * US_IN_MS), SMP_TERNARY(i, 0));
        SMP_COND_STATEMENT(NODE_STATE_ON_CORE(ksIdleThread, i)->tcbSchedContext->scCore = i;)
        if (!result) {
            printf("Kernel init failed: Unable to allocate sc for idle thread\n");
            return false;
        }
#endif
#ifdef ENABLE_SMP_SUPPORT
    }
#endif /* ENABLE_SMP_SUPPORT */
    return true;
}
/*Z 填充TCB CNode和tcb_t有关数据结构，创建initrd线程 */
BOOT_CODE tcb_t *create_initial_thread(cap_t root_cnode_cap, cap_t it_pd_cap, vptr_t ui_v_entry, vptr_t bi_frame_vptr,
                                       vptr_t ipcbuf_vptr, cap_t ipcbuf_cap)
{
    tcb_t *tcb = TCB_PTR(rootserver.tcb + TCB_OFFSET);
#ifndef CONFIG_KERNEL_MCS
    tcb->tcbTimeSlice = CONFIG_TIME_SLICE;
#endif
    /*Z 初始化上下文中cpu、FPU、DEBUG寄存器值 */
    Arch_initContext(&tcb->tcbArch.tcbContext);
    /*Z 拷贝一个IPC buffer能力 */
    /* derive a copy of the IPC buffer cap for inserting */
    deriveCap_ret_t dc_ret = deriveCap(SLOT_PTR(pptr_of_cap(root_cnode_cap), seL4_CapInitThreadIPCBuffer), ipcbuf_cap);
    if (dc_ret.status != EXCEPTION_NONE) {
        printf("Failed to derive copy of IPC Buffer\n");
        return NULL;
    }
    /*Z 在TCB CNode中插入根CNode、VSpace、IPC buffer访问能力，并与根CNode中的对应CSlot建立关联 */
    /* initialise TCB (corresponds directly to abstract specification) */
    cteInsert(
        root_cnode_cap,
        SLOT_PTR(pptr_of_cap(root_cnode_cap), seL4_CapInitThreadCNode),
        SLOT_PTR(rootserver.tcb, tcbCTable)
    );
    cteInsert(
        it_pd_cap,
        SLOT_PTR(pptr_of_cap(root_cnode_cap), seL4_CapInitThreadVSpace),
        SLOT_PTR(rootserver.tcb, tcbVTable)
    );
    cteInsert(
        dc_ret.cap,
        SLOT_PTR(pptr_of_cap(root_cnode_cap), seL4_CapInitThreadIPCBuffer),
        SLOT_PTR(rootserver.tcb, tcbBuffer)
    );
    tcb->tcbIPCBuffer = ipcbuf_vptr;
    /*Z 在TCB上下文中利用伪寄存器保存bootinfo和ELF入口点虚拟地址 */
    setRegister(tcb, capRegister, bi_frame_vptr);
    setNextPC(tcb, ui_v_entry);
    /*Z 配置SC，设置refill有关数据。注意：这里设置调度周期为0，即采用的是RoundRobin算法，而不是严格的周期限制 */
    /* initialise TCB */
#ifdef CONFIG_KERNEL_MCS
    if (!configure_sched_context(tcb, SC_PTR(rootserver.sc), usToTicks(CONFIG_BOOT_THREAD_TIME_SLICE * US_IN_MS), 0)) {
        return NULL;
    }
#endif
    /*Z 设置initrd优先级为最高，运行状态为可运行 */
    tcb->tcbPriority = seL4_MaxPrio;
    tcb->tcbMCP = seL4_MaxPrio;
#ifndef CONFIG_KERNEL_MCS
    setupReplyMaster(tcb);/*Z 如果线程未设置回复能力，则设置为主叫、允许授权回复、可撤销、首个标记 */
#endif
    setThreadState(tcb, ThreadState_Running);
    /*Z 保存当前活跃调度域及其剩余运行时间。不好：弄这么多同义变量，增加代码维护负担 */
    ksCurDomain = ksDomSchedule[ksDomScheduleIdx].domain;
#ifdef CONFIG_KERNEL_MCS
    ksDomainTime = usToTicks(ksDomSchedule[ksDomScheduleIdx].length * US_IN_MS);
#else
    ksDomainTime = ksDomSchedule[ksDomScheduleIdx].length;
#endif
    assert(ksCurDomain < CONFIG_NUM_DOMAINS && ksDomainTime > 0);

#ifndef CONFIG_KERNEL_MCS
    SMP_COND_STATEMENT(tcb->tcbAffinity = 0);
#endif
    /*Z 在根CNode中创建initrd TCB访问能力 */
    /* create initial thread's TCB cap */
    cap_t cap = cap_thread_cap_new(TCB_REF(tcb));
    write_slot(SLOT_PTR(pptr_of_cap(root_cnode_cap), seL4_CapInitThreadTCB), cap);

#ifdef CONFIG_KERNEL_MCS
    cap = cap_sched_context_cap_new(SC_REF(tcb->tcbSchedContext), seL4_MinSchedContextBits);
    write_slot(SLOT_PTR(pptr_of_cap(root_cnode_cap), seL4_CapInitThreadSC), cap);
#endif
#ifdef CONFIG_DEBUG_BUILD
    setThreadName(tcb, "rootserver");
#endif

    return tcb;
}
/*Z 初始化系统运行状态，设置调度器预选对象 */
BOOT_CODE void init_core_state(tcb_t *scheduler_action)
{   /*Z FPU寄存器状态置空 */
#ifdef CONFIG_HAVE_FPU
    NODE_STATE(ksActiveFPUState) = NULL;
#endif
#ifdef CONFIG_DEBUG_BUILD
    /* add initial threads to the debug queue */
    NODE_STATE(ksDebugTCBs) = NULL;/*Z 将线程插入到亲和cpu的DEBUG线程双向链表头 */
    if (scheduler_action != SchedulerAction_ResumeCurrentThread &&
        scheduler_action != SchedulerAction_ChooseNewThread) {
        tcbDebugAppend(scheduler_action);
    }/*Z idle线程也插入到亲和cpu的DEBUG线程双向链表头 */
    tcbDebugAppend(NODE_STATE(ksIdleThread));
#endif
    NODE_STATE(ksSchedulerAction) = scheduler_action;/*Z 将指定线程作为预选对象 */
    NODE_STATE(ksCurThread) = NODE_STATE(ksIdleThread);/*Z ksCurThread置为idle线程 */
#ifdef CONFIG_KERNEL_MCS    /*Z 当前调度上下文置为idle线程的SC */
    NODE_STATE(ksCurSC) = NODE_STATE(ksCurThread->tcbSchedContext);
    NODE_STATE(ksConsumed) = 0;
    NODE_STATE(ksReprogram) = true;
    NODE_STATE(ksReleaseHead) = NULL;
    NODE_STATE(ksCurTime) = getCurrentTime();
#endif
}
/*Z 将指定内存添加到全局untyped列表中，并在根CNode中创建相应的能力 */
BOOT_CODE static bool_t provide_untyped_cap(
    cap_t      root_cnode_cap,
    bool_t     device_memory,
    pptr_t     pptr,
    word_t     size_bits,
    seL4_SlotPos first_untyped_slot
)
{/*Z 这个函数隐含了全局数组索引递增的情况 */
    bool_t ret;
    cap_t ut_cap;
    word_t i = ndks_boot.slot_pos_cur - first_untyped_slot;/*Z 全局数组索引 */
    if (i < CONFIG_MAX_NUM_BOOTINFO_UNTYPED_CAPS) {
        ndks_boot.bi_frame->untypedList[i] = (seL4_UntypedDesc) {
            pptr_to_paddr((void *)pptr), size_bits, device_memory, {0}
        };
        ut_cap = cap_untyped_cap_new(MAX_FREE_INDEX(size_bits),/*Z 创建未映射内存控制能力。置上脏标记(无子CSlot是脏标记，否则是满标记) */
                                     device_memory, size_bits, pptr);
        ret = provide_cap(root_cnode_cap, ut_cap);/*Z 将能力写入根CNode */
    } else {
        printf("Kernel init: Too many untyped regions for boot info\n");
        ret = true;
    }
    return ret;
}
/*Z 将指定内存添加到bootinfo的untyped列表(地址、大小对齐，必要时分拆)，并在根CNode中创建相应的能力 */
BOOT_CODE bool_t create_untypeds_for_region(
    cap_t      root_cnode_cap,
    bool_t     device_memory,
    region_t   reg,
    seL4_SlotPos first_untyped_slot
)
{
    word_t align_bits;
    word_t size_bits;
    
    while (!is_reg_empty(reg)) {
        /* Determine the maximum size of the region */
        size_bits = seL4_WordBits - 1 - clzl(reg.end - reg.start);
        /*Z 如果区域大小的位数比对齐位数大，就按对齐大小分拆记录 */
        /* Determine the alignment of the region */
        if (reg.start != 0) {
            align_bits = ctzl(reg.start);
        } else {
            align_bits = size_bits;
        }
        /* Reduce size bits to align if needed */
        if (align_bits < size_bits) {
            size_bits = align_bits;
        }
        if (size_bits > seL4_MaxUntypedBits) {
            size_bits = seL4_MaxUntypedBits;
        }
        /*Z 将指定内存添加到全局untyped列表中，并在根CNode中创建相应的能力 */
        if (size_bits >= seL4_MinUntypedBits) {/*Z 不记录少于16字节的区域 */
            if (!provide_untyped_cap(root_cnode_cap, device_memory, reg.start, size_bits, first_untyped_slot)) {
                return false;
            }
        }
        reg.start += BIT(size_bits);
    }
    return true;
}
/*Z 将正常RAM以外的所有地址区域认定为设备内存(RAM或ROM，含1M以下正常RAM)，记录在bootinfo的untyped列表中，并在根CNode中创建相应的能力 */
BOOT_CODE bool_t create_device_untypeds(cap_t root_cnode_cap, seL4_SlotPos slot_pos_before)
{   /*Z 查找地址空间中正常RAM(包括被APIC、IOMMU占用的)以外的所有区域(包括1M以下的正常RAM)，认定为设备内存，记录在bootinfo列表中，并在根CNode中创建相应的能力 */
    paddr_t start = 0;
    for (word_t i = 0; i < ndks_boot.resv_count; i++) {/*Z 遍历保留的内存列表（含空闲区）*/
        if (start < ndks_boot.reserved[i].start) {
            region_t reg = paddr_to_pptr_reg((p_region_t) {/*Z 找列表间的区域 */
                start, ndks_boot.reserved[i].start
            });/*Z 认定为untyped设备内存，记录在bootinfo的untyped列表中，并在根CNode中创建相应的能力 */
            if (!create_untypeds_for_region(root_cnode_cap, true, reg, slot_pos_before)) {
                return false;
            }
        }

        start = ndks_boot.reserved[i].end;
    }
    /*Z 将保留区后面直到物理地址空间顶(47位地址空间)，也视为untyped的设备内存 */
    if (start < CONFIG_PADDR_USER_DEVICE_TOP) {
        region_t reg = paddr_to_pptr_reg((p_region_t) {
            start, CONFIG_PADDR_USER_DEVICE_TOP
        });
        /*
         * The auto-generated bitfield code will get upset if the
         * end pptr is larger than the maximum pointer size for this architecture.
         */
        if (reg.end > PPTR_TOP) {
            reg.end = PPTR_TOP;
        }
        if (!create_untypeds_for_region(root_cnode_cap, true, reg, slot_pos_before)) {
            return false;
        }
    }
    return true;
}
/*Z 将空闲区和内核启动代码区，记录在bootinfo的untyped列表中，并在根CNode中创建相应的能力 */
BOOT_CODE bool_t create_kernel_untypeds(cap_t root_cnode_cap, region_t boot_mem_reuse_reg,
                                        seL4_SlotPos first_untyped_slot)
{
    word_t     i;
    region_t   reg;
    /*Z 将内核启动代码区作为未映射内存，记录在bootinfo列表中，并在根CNode中创建相应的能力 */
    /* if boot_mem_reuse_reg is not empty, we can create UT objs from boot code/data frames */
    if (!create_untypeds_for_region(root_cnode_cap, false, boot_mem_reuse_reg, first_untyped_slot)) {
        return false;
    }
    /*Z 将空闲区记录在bootinfo列表中，并在根CNode中创建相应的能力 */
    /* convert remaining freemem into UT objects and provide the caps */
    for (i = 0; i < MAX_NUM_FREEMEM_REG; i++) {
        reg = ndks_boot.freemem[i];
        ndks_boot.freemem[i] = REG_EMPTY;
        if (!create_untypeds_for_region(root_cnode_cap, false, reg, first_untyped_slot)) {
            return false;
        }
    }

    return true;
}
/*Z 在即将启动结束前，记录根CNode中剩余的空闲CSlot */
BOOT_CODE void bi_finalise(void)
{
    seL4_SlotPos slot_pos_start = ndks_boot.slot_pos_cur;
    seL4_SlotPos slot_pos_end = ndks_boot.slot_pos_max;
    ndks_boot.bi_frame->empty = (seL4_SlotRegion) {
        slot_pos_start, slot_pos_end
    };
}
/*Z 如果参数代表的物理地址超过510G，则以510G封顶 */
static inline pptr_t ceiling_kernel_window(pptr_t p)
{
    /* Adjust address if it exceeds the kernel window
     * Note that we compare physical address in case of overflow.
     */
    if (pptr_to_paddr((void *)p) > PADDR_TOP) {
        p = PPTR_TOP;
    }
    return p;
}
/*Z 可用的物理内存区域线性地址表示。相当于本地临时变量，放在这应该与内核栈太小有关 */
/* we can't delcare arrays on the stack, so this is space for
 * the below function to use. */
static BOOT_DATA region_t avail_reg[MAX_NUM_FREEMEM_REG];
/**
 * Dynamically initialise the available memory on the platform.
 * A region represents an area of memory.
 */
/*Z 将initrd最终加载区域加入ndks_boot保留区，系统可用内存(除initrd占用的)加入ndks_boot保留区和空闲区；
在物理内存顶部为initrd线程分配rootserver数据结构内存。*/
BOOT_CODE void init_freemem(word_t n_available,         /*Z 可用内存计数 */
                            const p_region_t *available,/*Z 可用物理内存列表 */
                            word_t n_reserved,          /*Z 1 */
                            region_t *reserved,         /*Z initrd在内核空间中的区域 */
                            v_region_t it_v_reg,        /*Z initrd线程在自身虚拟地址空间的区域 */
                            word_t extra_bi_size_bits)  /*Z 额外的bootinfo大小（对数）*/
{   /*Z 要操作的区域必须按地址升序排列、无重叠。x86_64不执行这块 */
    /* Force ordering and exclusivity of reserved regions */
    for (word_t i = 0; n_reserved > 0 && i < n_reserved - 1; i++) {
        assert(reserved[i].start <= reserved[i].end);
        assert(reserved[i].end <= reserved[i + 1].start);
    }
    /*Z 不好：这个RAM图来自Multiboot，但其规范并没有保证RAM图不重叠且地址升序排列 */
    /* Force ordering and exclusivity of available regions */
    assert(n_available > 0);
    for (word_t i = 0; i < n_available - 1; i++) {
        assert(available[i].start < available[i].end);
        assert(available[i].end <= available[i + 1].start);
    }
    /*Z 将ndks_boot.freemem数组元素置空值 */
    for (word_t i = 0; i < MAX_NUM_FREEMEM_REG; i++) {
        ndks_boot.freemem[i] = REG_EMPTY;
    }
    /*Z 将可用物理内存转换为线性地址，暂存起来 */
    /* convert the available regions to pptrs */
    for (word_t i = 0; i < n_available; i++) {
        avail_reg[i] = paddr_to_pptr_reg(available[i]);
        avail_reg[i].end = ceiling_kernel_window(avail_reg[i].end);
        avail_reg[i].start = ceiling_kernel_window(avail_reg[i].start);
    }
    /*Z 填充ndks_boot的保留区和空闲区：要保留的加入保留区，不交叉的可用区加入保留区和空闲区 */
    word_t a = 0;
    word_t r = 0;
    /* Now iterate through the available regions, removing any reserved regions. */
    while (a < n_available && r < n_reserved) {
        if (reserved[r].start == reserved[r].end) {/*Z 跳过空保留区 */
            /* reserved region is empty - skip it */
            r++;
        } else if (avail_reg[a].start >= avail_reg[a].end) {/*Z 跳过空可用区 */
            /* skip the entire region - it's empty now after trimming */
            a++;
        } else if (reserved[r].end <= avail_reg[a].start) {/*Z 在左侧时，将要保留的加入到ndks_boot的保留区 */
            /* the reserved region is below the available region - skip it*/
            reserve_region(pptr_to_paddr_reg(reserved[r]));
            r++;
        } else if (reserved[r].start >= avail_reg[a].end) {/*Z 在右侧时，将可用的加入到ndks_boot的保留区和free区 */
            /* the reserved region is above the available region - take the whole thing */
            insert_region(avail_reg[a]);
            a++;
        } else {
            /* the reserved region overlaps with the available region */
            if (reserved[r].start <= avail_reg[a].start) {/*Z 左交叉时，缩小可用的，将要保留的加入到ndks_boot的保留区 */
                /* the region overlaps with the start of the available region.
                 * trim start of the available region */
                avail_reg[a].start = MIN(avail_reg[a].end, reserved[r].end);
                reserve_region(pptr_to_paddr_reg(reserved[r]));
                r++;
            } else {/*Z 中或右交叉时 */
                assert(reserved[r].start < avail_reg[a].end);
                /* take the first chunk of the available region and move
                 * the start to the end of the reserved region */
                region_t m = avail_reg[a];
                m.end = reserved[r].start;
                insert_region(m);/*Z 将可用的左部分加入到ndks_boot的保留区和free区 */
                if (avail_reg[a].end > reserved[r].end) {/*Z 中包含时，将要保留的加入到ndks_boot的保留区 */
                    avail_reg[a].start = reserved[r].end;
                    reserve_region(pptr_to_paddr_reg(reserved[r]));
                    r++;
                } else {/*Z 右交叉时，待下一个可用的再处理 */
                    a++;
                }
            }
        }
    }
    /*Z 填充ndks_boot的保留区和空闲区：剩余的要保留的加入保留区 */
    for (; r < n_reserved; r++) {
        if (reserved[r].start < reserved[r].end) {
            reserve_region(pptr_to_paddr_reg(reserved[r]));
        }
    }
    /*Z 填充ndks_boot的保留区和空闲区：剩余的可用区加入保留区和空闲区 */
    /* no more reserved regions - add the rest */
    for (; a < n_available; a++) {
        if (avail_reg[a].start < avail_reg[a].end) {
            insert_region(avail_reg[a]);
        }
    }
    /*Z 从后往前找一个非空的空闲区 */
    /* now try to fit the root server objects into a region */
    word_t i = MAX_NUM_FREEMEM_REG - 1;             /*Z 不好：无符号的i在后面>=0条件中都没用 */
    if (!is_reg_empty(ndks_boot.freemem[i])) {
        printf("Insufficient MAX_NUM_FREEMEM_REG");
        halt();
    }
    /* skip any empty regions */
    for (; is_reg_empty(ndks_boot.freemem[i]) && i >= 0; i--);

    /* try to grab the last available p region to create the root server objects
     * from. If possible, retain any left over memory as an extra p region */
    word_t size = calculate_rootserver_size(it_v_reg, extra_bi_size_bits);/*Z 计算rootserver(initrd)还需要分配的内存 */
    word_t max = rootserver_max_size_bits(extra_bi_size_bits);/*Z 计算额外bootinfo、CNode、VSpace中的最大值(位数表示) */
    for (; i >= 0; i--) {/*Z 从后往前，找第一个足够大的可用空闲区 */
        word_t next = i + 1;
        pptr_t start = ROUND_DOWN(ndks_boot.freemem[i].end - size, max);/*Z 按max取齐是为了从大到小分配内存以利对齐 */
        if (start >= ndks_boot.freemem[i].start) {/*Z 找到了 */
            create_rootserver_objects(start, it_v_reg, extra_bi_size_bits);/*Z 分配所需的rootserver内存 */
            if (i < MAX_NUM_FREEMEM_REG) {/*Z 不好：这个if废话，下面的也是 */
                ndks_boot.freemem[next].end = ndks_boot.freemem[i].end;/*Z 留着取齐剩下的一点空闲内存 */
                ndks_boot.freemem[next].start = start + size;
            }
            ndks_boot.freemem[i].end = start;
            break;
        } else if (i < MAX_NUM_FREEMEM_REG) {
            ndks_boot.freemem[next] = ndks_boot.freemem[i];
        }
    }
}
