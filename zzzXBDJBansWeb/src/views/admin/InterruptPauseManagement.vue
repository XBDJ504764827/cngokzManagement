<script setup>
import { computed, onMounted, ref } from 'vue'
import api from '@/utils/api'
import { useToast } from '@/composables/useToast'

const toast = useToast()

const snapshots = ref([])
const loading = ref(false)
const activeFilter = ref('all')
const showRejectModal = ref(false)
const rejectReason = ref('')
const rejectTarget = ref(null)

const fetchSnapshots = async () => {
  loading.value = true
  try {
    const res = await api.get('/interrupt-pause')
    snapshots.value = res.data
  } catch (err) {
    console.error(err)
    toast.error('加载中断暂停列表失败')
  } finally {
    loading.value = false
  }
}

const stats = computed(() => {
  const total = snapshots.value.length
  const pending = snapshots.value.filter(item => item.restore_status === 'pending').length
  const approved = snapshots.value.filter(item => item.restore_status === 'approved').length
  const rejected = snapshots.value.filter(item => item.restore_status === 'rejected').length

  return { total, pending, approved, rejected }
})

const filters = [
  { key: 'all', label: '全部记录' },
  { key: 'pending', label: '待审核' },
  { key: 'approved', label: '已授权' },
  { key: 'rejected', label: '已拒绝' },
  { key: 'none', label: '未申请' },
  { key: 'restored', label: '已恢复' },
  { key: 'aborted', label: '已终止' },
]

const filteredSnapshots = computed(() => {
  if (activeFilter.value === 'all') {
    return snapshots.value
  }
  return snapshots.value.filter(item => item.restore_status === activeFilter.value)
})

const getStatusLabel = (status) => {
  const map = {
    none: '未申请',
    pending: '待审核',
    approved: '已授权',
    rejected: '已拒绝',
    restored: '已恢复',
    aborted: '已终止',
  }
  return map[status] || status
}

const getStatusClass = (status) => {
  const map = {
    none: 'bg-slate-100 text-slate-700 border-slate-200 dark:bg-slate-800 dark:text-slate-300 dark:border-slate-700',
    pending: 'bg-amber-50 text-amber-700 border-amber-200 dark:bg-amber-500/10 dark:text-amber-300 dark:border-amber-500/20',
    approved: 'bg-emerald-50 text-emerald-700 border-emerald-200 dark:bg-emerald-500/10 dark:text-emerald-300 dark:border-emerald-500/20',
    rejected: 'bg-rose-50 text-rose-700 border-rose-200 dark:bg-rose-500/10 dark:text-rose-300 dark:border-rose-500/20',
    restored: 'bg-blue-50 text-blue-700 border-blue-200 dark:bg-blue-500/10 dark:text-blue-300 dark:border-blue-500/20',
    aborted: 'bg-slate-100 text-slate-600 border-slate-200 dark:bg-slate-800 dark:text-slate-400 dark:border-slate-700',
  }
  return map[status] || map.none
}

const formatRunTime = (seconds) => {
  const safeSeconds = Number(seconds || 0)
  const totalMilliseconds = Math.max(0, Math.floor(safeSeconds * 1000))
  const minutes = Math.floor(totalMilliseconds / 60000)
  const secs = Math.floor((totalMilliseconds % 60000) / 1000)
  const milliseconds = totalMilliseconds % 1000
  return `${String(minutes).padStart(2, '0')}:${String(secs).padStart(2, '0')}.${String(milliseconds).padStart(3, '0')}`
}

const formatDateTime = (value) => {
  if (!value) return '-'
  return new Date(value).toLocaleString()
}

const openRejectModal = (item) => {
  rejectTarget.value = item
  rejectReason.value = ''
  showRejectModal.value = true
}

const handleApprove = async (item) => {
  try {
    await api.put(`/interrupt-pause/${item.id}/approve`)
    toast.success('已授权恢复')
    fetchSnapshots()
  } catch (err) {
    console.error(err)
    toast.error(err.response?.data?.error || '授权失败')
  }
}

const confirmReject = async () => {
  if (!rejectTarget.value) return
  if (!rejectReason.value.trim()) {
    toast.error('请填写拒绝理由')
    return
  }

  try {
    await api.put(`/interrupt-pause/${rejectTarget.value.id}/reject`, {
      reason: rejectReason.value.trim(),
    })
    toast.success('已拒绝恢复申请')
    showRejectModal.value = false
    rejectTarget.value = null
    rejectReason.value = ''
    fetchSnapshots()
  } catch (err) {
    console.error(err)
    toast.error(err.response?.data?.error || '拒绝失败')
  }
}

onMounted(fetchSnapshots)
</script>

<template>
  <div class="space-y-6">
    <div class="flex flex-col gap-4 lg:flex-row lg:items-center lg:justify-between">
      <div>
        <h1 class="text-2xl font-bold text-slate-900 dark:text-white">中断暂停授权</h1>
        <p class="mt-1 text-sm text-slate-500 dark:text-slate-400">
          查看所有玩家中断暂停存档，并审核恢复申请。
        </p>
      </div>
      <button
        @click="fetchSnapshots"
        class="inline-flex items-center justify-center gap-2 rounded-lg border border-slate-200 bg-white px-4 py-2 text-sm font-medium text-slate-700 transition-colors hover:bg-slate-50 dark:border-slate-700 dark:bg-slate-900 dark:text-slate-200 dark:hover:bg-slate-800"
      >
        刷新列表
      </button>
    </div>

    <div class="grid gap-4 md:grid-cols-2 xl:grid-cols-4">
      <div class="rounded-xl border border-gray-200 bg-white p-5 shadow-sm dark:border-slate-800 dark:bg-slate-900">
        <p class="text-sm text-slate-500 dark:text-slate-400">总记录</p>
        <p class="mt-2 text-3xl font-semibold text-slate-900 dark:text-white">{{ stats.total }}</p>
      </div>
      <div class="rounded-xl border border-gray-200 bg-white p-5 shadow-sm dark:border-slate-800 dark:bg-slate-900">
        <p class="text-sm text-slate-500 dark:text-slate-400">待审核</p>
        <p class="mt-2 text-3xl font-semibold text-amber-600 dark:text-amber-300">{{ stats.pending }}</p>
      </div>
      <div class="rounded-xl border border-gray-200 bg-white p-5 shadow-sm dark:border-slate-800 dark:bg-slate-900">
        <p class="text-sm text-slate-500 dark:text-slate-400">已授权</p>
        <p class="mt-2 text-3xl font-semibold text-emerald-600 dark:text-emerald-300">{{ stats.approved }}</p>
      </div>
      <div class="rounded-xl border border-gray-200 bg-white p-5 shadow-sm dark:border-slate-800 dark:bg-slate-900">
        <p class="text-sm text-slate-500 dark:text-slate-400">已拒绝</p>
        <p class="mt-2 text-3xl font-semibold text-rose-600 dark:text-rose-300">{{ stats.rejected }}</p>
      </div>
    </div>

    <div class="flex flex-wrap gap-2 rounded-xl border border-gray-200 bg-white p-2 shadow-sm dark:border-slate-800 dark:bg-slate-900">
      <button
        v-for="item in filters"
        :key="item.key"
        @click="activeFilter = item.key"
        class="rounded-lg border px-4 py-2 text-sm font-medium transition-all"
        :class="activeFilter === item.key
          ? 'border-blue-200 bg-blue-50 text-blue-600 shadow-sm dark:border-blue-500/20 dark:bg-blue-500/10 dark:text-blue-300'
          : 'border-transparent bg-transparent text-slate-500 hover:bg-gray-100 hover:text-slate-800 dark:text-slate-400 dark:hover:bg-slate-800/70 dark:hover:text-slate-200'"
      >
        {{ item.label }}
      </button>
    </div>

    <div class="overflow-hidden rounded-xl border border-gray-200 bg-white shadow-sm dark:border-slate-800 dark:bg-slate-900">
      <div class="overflow-x-auto">
        <table class="min-w-full divide-y divide-slate-200 dark:divide-slate-800">
          <thead class="bg-gray-50 dark:bg-slate-950/50">
            <tr>
              <th class="px-5 py-3 text-left text-xs font-semibold uppercase tracking-wider text-slate-500 dark:text-slate-400">玩家</th>
              <th class="px-5 py-3 text-left text-xs font-semibold uppercase tracking-wider text-slate-500 dark:text-slate-400">服务器 / 地图</th>
              <th class="px-5 py-3 text-left text-xs font-semibold uppercase tracking-wider text-slate-500 dark:text-slate-400">进度</th>
              <th class="px-5 py-3 text-left text-xs font-semibold uppercase tracking-wider text-slate-500 dark:text-slate-400">状态</th>
              <th class="px-5 py-3 text-left text-xs font-semibold uppercase tracking-wider text-slate-500 dark:text-slate-400">审核信息</th>
              <th class="px-5 py-3 text-left text-xs font-semibold uppercase tracking-wider text-slate-500 dark:text-slate-400">保存时间</th>
              <th class="px-5 py-3 text-right text-xs font-semibold uppercase tracking-wider text-slate-500 dark:text-slate-400">操作</th>
            </tr>
          </thead>
          <tbody class="divide-y divide-slate-200 dark:divide-slate-800">
            <tr v-if="loading">
              <td colspan="7" class="px-5 py-12 text-center text-sm text-slate-500 dark:text-slate-400">
                正在加载中断暂停记录...
              </td>
            </tr>
            <tr v-else-if="filteredSnapshots.length === 0">
              <td colspan="7" class="px-5 py-12 text-center text-sm text-slate-500 dark:text-slate-400">
                当前筛选条件下暂无记录
              </td>
            </tr>
            <tr
              v-for="item in filteredSnapshots"
              :key="item.id"
              class="align-top transition-colors hover:bg-slate-50 dark:hover:bg-slate-800/40"
            >
              <td class="px-5 py-4">
                <div class="space-y-1">
                  <p class="font-semibold text-slate-900 dark:text-white">{{ item.player_name }}</p>
                  <p class="font-mono text-xs text-slate-500 dark:text-slate-400">{{ item.auth_steamid64 || item.auth_primary }}</p>
                  <p class="font-mono text-xs text-slate-400 dark:text-slate-500">IP: {{ item.ip_address }}</p>
                </div>
              </td>
              <td class="px-5 py-4">
                <div class="space-y-1 text-sm">
                  <p class="font-medium text-slate-800 dark:text-slate-200">{{ item.server_name || `#${item.server_id}` }}</p>
                  <p class="font-mono text-slate-500 dark:text-slate-400">{{ item.map_name }}</p>
                  <p class="text-xs text-slate-400 dark:text-slate-500">模式 {{ item.mode }} / 赛道 {{ item.course }}</p>
                </div>
              </td>
              <td class="px-5 py-4">
                <div class="space-y-1 text-sm text-slate-600 dark:text-slate-300">
                  <p class="font-semibold text-slate-900 dark:text-white">{{ formatRunTime(item.time_seconds) }}</p>
                  <p>Checkpoint: {{ item.checkpoint_count }}</p>
                  <p>Teleport: {{ item.teleport_count }}</p>
                </div>
              </td>
              <td class="px-5 py-4">
                <span
                  class="inline-flex rounded-full border px-2.5 py-1 text-xs font-semibold"
                  :class="getStatusClass(item.restore_status)"
                >
                  {{ getStatusLabel(item.restore_status) }}
                </span>
              </td>
              <td class="px-5 py-4">
                <div class="space-y-1 text-sm">
                  <p class="text-slate-600 dark:text-slate-300">
                    申请时间: {{ formatDateTime(item.restore_requested_at) }}
                  </p>
                  <p class="text-slate-600 dark:text-slate-300">
                    审核人: {{ item.reviewed_by || '-' }}
                  </p>
                  <p class="text-slate-600 dark:text-slate-300">
                    审核时间: {{ formatDateTime(item.reviewed_at) }}
                  </p>
                  <p v-if="item.reject_reason" class="max-w-sm text-sm text-rose-600 dark:text-rose-300">
                    拒绝理由: {{ item.reject_reason }}
                  </p>
                  <p v-if="item.restored_at" class="text-slate-600 dark:text-slate-300">
                    恢复时间: {{ formatDateTime(item.restored_at) }}
                  </p>
                </div>
              </td>
              <td class="px-5 py-4 text-sm text-slate-500 dark:text-slate-400">
                <div>创建: {{ formatDateTime(item.created_at) }}</div>
                <div>更新: {{ formatDateTime(item.updated_at) }}</div>
              </td>
              <td class="px-5 py-4">
                <div class="flex justify-end gap-2" v-if="item.restore_status === 'pending'">
                  <button
                    @click="handleApprove(item)"
                    class="rounded-lg bg-emerald-600 px-3 py-2 text-sm font-medium text-white transition-colors hover:bg-emerald-500"
                  >
                    通过
                  </button>
                  <button
                    @click="openRejectModal(item)"
                    class="rounded-lg bg-rose-600 px-3 py-2 text-sm font-medium text-white transition-colors hover:bg-rose-500"
                  >
                    拒绝
                  </button>
                </div>
                <div v-else class="text-right text-xs text-slate-400 dark:text-slate-500">
                  无可执行操作
                </div>
              </td>
            </tr>
          </tbody>
        </table>
      </div>
    </div>

    <div
      v-if="showRejectModal"
      class="fixed inset-0 z-50 flex items-center justify-center bg-slate-950/60 px-4"
      @click.self="showRejectModal = false"
    >
      <div class="w-full max-w-lg rounded-2xl border border-slate-200 bg-white p-6 shadow-2xl dark:border-slate-800 dark:bg-slate-900">
        <h3 class="text-lg font-semibold text-slate-900 dark:text-white">拒绝恢复申请</h3>
        <p class="mt-2 text-sm text-slate-500 dark:text-slate-400">
          请输入拒绝理由，玩家端会展示这条说明。
        </p>
        <textarea
          v-model="rejectReason"
          rows="5"
          class="mt-4 w-full rounded-xl border border-slate-300 bg-white px-4 py-3 text-sm text-slate-900 outline-none transition-colors focus:border-blue-500 dark:border-slate-700 dark:bg-slate-950 dark:text-white"
          placeholder="例如：检测到 IP 与存档保存时不一致，请联系管理员处理。"
        />
        <div class="mt-5 flex justify-end gap-3">
          <button
            @click="showRejectModal = false"
            class="rounded-lg border border-slate-300 px-4 py-2 text-sm font-medium text-slate-700 transition-colors hover:bg-slate-50 dark:border-slate-700 dark:text-slate-200 dark:hover:bg-slate-800"
          >
            取消
          </button>
          <button
            @click="confirmReject"
            class="rounded-lg bg-rose-600 px-4 py-2 text-sm font-medium text-white transition-colors hover:bg-rose-500"
          >
            确认拒绝
          </button>
        </div>
      </div>
    </div>
  </div>
</template>
