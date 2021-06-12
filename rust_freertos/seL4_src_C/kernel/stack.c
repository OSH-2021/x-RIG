/*Z OK 内核栈定义 */

/*
 * Copyright 2020, Data61, CSIRO (ABN 41 687 119 230)
 *
 * SPDX-License-Identifier: GPL-2.0-only
 */

#include <kernel/stack.h>
/*Z 静态定义的内核栈，每node一个，每个4KB大小。最后一个节点的初始栈指针指向本结构后第一个字节位置 */
VISIBLE ALIGN(KERNEL_STACK_ALIGNMENT)
char kernel_stack_alloc[CONFIG_MAX_NUM_NODES][BIT(CONFIG_KERNEL_STACK_BITS)];
