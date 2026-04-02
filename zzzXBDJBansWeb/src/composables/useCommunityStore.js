import { ref, computed } from 'vue'
import api from '../utils/api'

// Global state
const serverGroups = ref([])

export const useCommunityStore = () => {
    const mapServerGroup = (groups) => groups.map(g => ({
        ...g,
        servers: g.servers.map(s => ({
            ...s,
            status: s.status || 'unknown'
        }))
    }))

    // Fetch
    const fetchServerGroups = async () => {
        try {
            const res = await api.get('/server-groups')
            serverGroups.value = mapServerGroup(res.data)
            await refreshServerStatuses()

        } catch (e) {
            console.error(e)
        }
    }

    const refreshServerStatuses = async () => {
        try {
            const res = await api.get('/server-statuses')
            const statuses = new Map(res.data.map(item => [item.server_id, item.status]))

            for (const group of serverGroups.value) {
                for (const server of group.servers) {
                    server.status = statuses.get(server.id) || 'unknown'
                }
            }
        } catch (e) {
            console.error(e)
        }
    }

    const updateLocalStatus = (groupId, serverId, status) => {
        const group = serverGroups.value.find(g => g.id === groupId)
        if (group) {
            const s = group.servers.find(s => s.id === serverId)
            if (s) s.status = status
        }
    }

    // Server Groups
    const addServerGroup = async (name) => {
        try {
            await api.post('/server-groups', { name })
            await fetchServerGroups()
            return { success: true }
        } catch (e) {
            return { success: false, message: e.response?.data || '创建失败' }
        }
    }

    const removeServerGroup = async (groupId) => {
        try {
            await api.delete(`/server-groups/${groupId}`)
            await fetchServerGroups()
            return { success: true }
        } catch (e) {
            return { success: false, message: '删除失败' }
        }
    }

    // Servers
    const addServer = async (groupId, serverData) => {
        try {
            await api.post('/servers', { group_id: groupId, ...serverData })
            await fetchServerGroups()
            return { success: true }
        } catch (e) {
            return { success: false, message: e.response?.data || '添加失败' }
        }
    }

    const updateServer = async (groupId, serverId, serverData) => {
        try {
            const payload = { ...serverData }
            if (!payload.rcon_password) {
                delete payload.rcon_password
            }
            await api.put(`/servers/${serverId}`, payload)
            await fetchServerGroups()
            return { success: true }
        } catch (e) {
            return { success: false, message: e.response?.data || '更新失败' }
        }
    }

    const removeServer = async (groupId, serverId) => {
        try {
            await api.delete(`/servers/${serverId}`)
            await fetchServerGroups()
            return { success: true }
        } catch (e) {
            return { success: false, message: '删除失败' }
        }
    }

    // Check Server Status (RCON)
    const checkServer = async (connectionInfo) => {
        try {
            // connectionInfo: { ip, port, rcon_password }
            await api.post('/servers/check', connectionInfo)
            return { success: true }
        } catch (e) {
            return { success: false, message: e.response?.data || '连接失败' }
        }
    }

    // Players
    const fetchPlayers = async (serverId) => {
        try {
            const res = await api.get(`/servers/${serverId}/players`)
            return { success: true, data: res.data }
        } catch (e) {
            return { success: false, message: e.response?.data || '获取玩家列表失败' }
        }
    }

    const kickPlayer = async (serverId, userid, reason) => {
        try {
            await api.post(`/servers/${serverId}/kick`, { userid, reason })
            return { success: true }
        } catch (e) {
            return { success: false, message: e.response?.data || '踢出失败' }
        }
    }

    const banPlayer = async (serverId, userid, duration, reason) => {
        try {
            await api.post(`/servers/${serverId}/ban`, { userid, duration, reason })
            return { success: true }
        } catch (e) {
            return { success: false, message: e.response?.data || '封禁失败' }
        }
    }

    // Getters / Computed
    const hasCommunity = computed(() => serverGroups.value.length > 0)

    return {
        serverGroups,
        hasCommunity,
        fetchServerGroups,
        refreshServerStatuses,
        addServerGroup,
        removeServerGroup,
        addServer,
        updateServer,
        removeServer,
        checkServer,
        fetchPlayers,
        kickPlayer,
        banPlayer
    }
}
