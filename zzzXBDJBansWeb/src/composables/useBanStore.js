import { ref } from 'vue'
import api from '../utils/api'

// State
const bans = ref([])
const publicBans = ref([])
const banPagination = ref({
    page: 1,
    pageSize: 25,
    total: 0
})
const publicBanPagination = ref({
    page: 1,
    pageSize: 25,
    total: 0,
    status: 'active',
    search: ''
})

export const useBanStore = () => {

    const mapBanFromBackend = (b) => ({
        id: b.id,
        name: b.name,
        steamId: b.steam_id,
        steam_id_3: b.steam_id_3,
        steam_id_64: b.steam_id_64,
        ip: b.ip,
        banType: b.ban_type, // "account" or "ip"
        reason: b.reason,
        duration: b.duration,
        status: b.status,
        adminName: b.admin_name,
        createTime: b.created_at,
        expiresAt: b.expires_at,
        serverId: b.server_id
    })

    const fetchBans = async ({ page = banPagination.value.page, pageSize = banPagination.value.pageSize } = {}) => {
        try {
            const res = await api.get('/bans', {
                params: {
                    page,
                    page_size: pageSize
                }
            })
            bans.value = res.data.items.map(mapBanFromBackend)
            banPagination.value = {
                page: res.data.page,
                pageSize: res.data.page_size,
                total: res.data.total
            }
        } catch (e) {
            console.error(e)
        }
    }

    const fetchPublicBans = async ({
        page = publicBanPagination.value.page,
        pageSize = publicBanPagination.value.pageSize,
        status = publicBanPagination.value.status,
        search = publicBanPagination.value.search
    } = {}) => {
        try {
            const params = {
                page,
                page_size: pageSize
            }

            if (status && status !== 'all') {
                params.status = status
            }

            if (search) {
                params.search = search
            }

            const res = await api.get('/bans/public', { params })
            publicBans.value = res.data.items.map(mapBanFromBackend)
            publicBanPagination.value = {
                page: res.data.page,
                pageSize: res.data.page_size,
                total: res.data.total,
                status,
                search: search || ''
            }
        } catch (e) {
            console.error(e)
        }
    }

    const addBan = async (banData) => {
        try {
            // Frontend banData matches backend EXPECTED payload?
            // Backend `CreateBanRequest`:
            // name, steam_id, ip, ban_type, reason, duration, admin_name
            // Frontend sends camelCase. We need to convert.
            const payload = {
                name: banData.name,
                steam_id: banData.steamId,
                ip: banData.ip,
                ban_type: banData.banType,
                reason: banData.reason,
                duration: banData.duration,
                admin_name: banData.adminName
            }
            const res = await api.post('/bans', payload)
            const createdBan = mapBanFromBackend(res.data)
            banPagination.value.total += 1
            if (banPagination.value.page === 1) {
                bans.value = [createdBan, ...bans.value].slice(0, banPagination.value.pageSize)
            }
            return { success: true }
        } catch (e) {
            console.error(e)
            return { success: false, message: e.response?.data || 'Failed' }
        }
    }

    const removeBan = async (id) => {
        // "Unban" -> Update status to 'unbanned'
        try {
            // Backend `UpdateBanRequest` accepts `status`.
            // Our previous Logic was `removeBan` implies unbanning.
            // Backend has `delete` handler too, but that's HARD delete.
            // "解除封禁" usually means setting status to unbanned.
            // Let's use PUT update status.
            const res = await api.put(`/bans/${id}`, { status: 'unbanned' })
            const updatedBan = mapBanFromBackend(res.data)
            bans.value = bans.value.map(item => item.id === id ? updatedBan : item)
            return true
        } catch (e) {
            console.error(e)
            return false
        }
    }

    const updateBan = async (id, updatedData) => {
        try {
            // Map frontend fields to backend fields if necessary
            // updatedData might contain reason, duration, etc.
            const payload = {}
            if (updatedData.status) payload.status = updatedData.status
            if (updatedData.name) payload.name = updatedData.name
            if (updatedData.steamId) payload.steam_id = updatedData.steamId
            if (updatedData.ip) payload.ip = updatedData.ip
            if (updatedData.banType) payload.ban_type = updatedData.banType
            if (updatedData.reason) payload.reason = updatedData.reason
            if (updatedData.duration) payload.duration = updatedData.duration

            const res = await api.put(`/bans/${id}`, payload)
            const updatedBan = mapBanFromBackend(res.data)
            bans.value = bans.value.map(item => item.id === id ? updatedBan : item)
            return true
        } catch (e) {
            console.error(e)
            return false
        }
    }

    // Hard delete (Super Admin only)
    const deleteBanRecord = async (id) => {
        try {
            await api.delete(`/bans/${id}`)
            const hadRecord = bans.value.some(item => item.id === id)
            bans.value = bans.value.filter(item => item.id !== id)
            if (hadRecord) {
                banPagination.value.total = Math.max(0, banPagination.value.total - 1)
                if (bans.value.length === 0 && banPagination.value.page > 1) {
                    await fetchBans({ page: banPagination.value.page - 1 })
                } else {
                    const visibleCapacityBefore = banPagination.value.page * banPagination.value.pageSize
                    if (bans.value.length < banPagination.value.pageSize && banPagination.value.total >= visibleCapacityBefore) {
                        await fetchBans({ page: banPagination.value.page })
                    }
                }
            }
            return true
        } catch (e) {
            console.error(e)
            return false
        }
    }

    return {
        bans,
        publicBans,
        banPagination,
        publicBanPagination,
        fetchBans,
        fetchPublicBans,
        addBan,
        removeBan,
        updateBan,
        deleteBanRecord
    }
}
