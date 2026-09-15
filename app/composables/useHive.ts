import { invoke } from '@tauri-apps/api/core'
import { openUrl } from '@tauri-apps/plugin-opener'

export type ServiceId = 'opencode' | 'codex'

export interface ServiceStatus {
  opencode: boolean
  codex: boolean
}

export interface HiveModel {
  id: string
  model_type: string
}

export const SERVICE_META: Record<
  ServiceId,
  {
    title: string
    description: string
    icon: string
    downloadUrl: string
  }
> = {
  opencode: {
    title: 'OpenCode',
    description: "IDE di coding di OpenCode. Scarica e installa l'app, poi riprova.",
    icon: 'i-simple-icons-opencode',
    downloadUrl: 'https://opencode.ai/download'
  },
  codex: {
    title: 'Codex',
    description: 'CLI di coding di OpenAI. Usa un profilo dedicato puntato ad AI Hive.',
    icon: 'i-simple-icons-openai',
    downloadUrl: 'https://chatgpt.com/codex'
  }
}

const isTauri = computed(() => typeof window !== 'undefined' && '__TAURI_INTERNALS__' in window)

const key = ref('')
const keySaved = ref(false)
const saving = ref(false)
const models = ref<HiveModel[]>([])
const selectedModel = ref('')

// Only TEXT_GENERATION models make sense as an LLM; fall back to the full
// list if the gateway ever stops reporting the type.
const chatModels = computed(() => {
  const text = models.value.filter(m => m.model_type === 'TEXT_GENERATION')
  return text.length > 0 ? text : models.value
})
const chatModelIds = computed(() => chatModels.value.map(m => m.id))
const status = ref<ServiceStatus | null>(null)
const refreshing = ref(false)
const launching = ref<ServiceId | null>(null)
const keyModalOpen = ref(false)

export function useHive() {
  const toast = useToast()

  async function refreshStatus() {
    if (!isTauri.value || refreshing.value) return
    refreshing.value = true
    try {
      status.value = await invoke<ServiceStatus>('check_services')
    } catch {
      // stato non determinante: mantieni i valori attuali
    } finally {
      refreshing.value = false
    }
  }

  async function loadSavedKey() {
    if (!isTauri.value) return
    try {
      const saved = await invoke<string | null>('get_hive_key')
      if (saved) {
        key.value = saved
        keySaved.value = true
        await loadModels()
      }
    } catch {
      // chiave assente o non leggibile: ignora
    }
  }

  async function loadModels() {
    if (!key.value.trim()) return
    try {
      models.value = await invoke<HiveModel[]>('list_hive_models', {
        key: key.value.trim()
      })
      const first = chatModelIds.value[0]
      if (first && !chatModelIds.value.includes(selectedModel.value)) {
        selectedModel.value = first
      }
    } catch (error) {
      toast.add({
        title: 'Impossibile caricare i modelli da AI Hive',
        description: String(error),
        color: 'error'
      })
    }
  }

  async function saveKey(): Promise<boolean> {
    const trimmed = key.value.trim()
    if (!trimmed || saving.value) return false
    saving.value = true
    try {
      await invoke('set_hive_key', { key: trimmed })
      key.value = trimmed
      keySaved.value = true
      await loadModels()
      toast.add({
        title: 'Chiave salvata e verificata',
        description: `${models.value.length} modelli disponibili su AI Hive.`,
        color: 'success'
      })
      return true
    } catch (error) {
      toast.add({
        title: 'Salvataggio non riuscito',
        description: String(error),
        color: 'error'
      })
      return false
    } finally {
      saving.value = false
    }
  }

  async function clearKey() {
    if (isTauri.value) {
      try {
        await invoke('delete_hive_key')
      } catch {
        // il file potrebbe già non esistere
      }
    }
    key.value = ''
    keySaved.value = false
    models.value = []
    selectedModel.value = ''
  }

  async function launch(service: ServiceId) {
    if (!key.value.trim() || !selectedModel.value || launching.value) return
    launching.value = service
    try {
      await invoke('launch_service', {
        service,
        model: selectedModel.value,
        key: key.value.trim()
      })
      toast.add({
        title: `${SERVICE_META[service].title} avviato`,

        color: 'success'
      })
      await refreshStatus()
    } catch (error) {
      toast.add({
        title: `Avvio di ${SERVICE_META[service].title} non riuscito`,
        description: String(error),
        color: 'error'
      })
    } finally {
      launching.value = null
    }
  }

  async function openExternal(url: string) {
    if (isTauri.value) {
      await openUrl(url)
    } else {
      window.open(url, '_blank', 'noopener,noreferrer')
    }
  }

  return {
    isTauri,
    key,
    keySaved,
    saving,
    models,
    selectedModel,
    status,
    refreshing,
    launching,
    keyModalOpen,
    chatModels,
    chatModelIds,
    refreshStatus,
    loadSavedKey,
    saveKey,
    clearKey,
    launch,
    openExternal
  }
}
