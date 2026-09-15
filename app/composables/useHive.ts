import { invoke } from '@tauri-apps/api/core'

export type ServiceId = 'opencode' | 'codex'

export interface ServiceStatus {
  opencode: boolean
  codex: boolean
}

export const SERVICE_META: Record<ServiceId, {
  title: string
  description: string
  icon: string
  installCommand: string
}> = {
  opencode: {
    title: 'OpenCode',
    description: 'Agente di coding in terminale. Si avvia con configurazione inline puntata ad AI Hive.',
    icon: 'i-simple-icons-opencode',
    installCommand: 'npm i -g opencode-ai'
  },
  codex: {
    title: 'Codex',
    description: 'CLI di coding di OpenAI. Usa un profilo dedicato (~/.codex/hive.config.toml) puntato ad AI Hive.',
    icon: 'i-simple-icons-openai',
    installCommand: 'npm i -g @openai/codex'
  }
}

export function useHive() {
  const toast = useToast()

  const isTauri = computed(() => typeof window !== 'undefined' && '__TAURI_INTERNALS__' in window)

  const key = ref('')
  const keySaved = ref(false)
  const saving = ref(false)
  const models = ref<string[]>([])
  const selectedModel = ref('')
  const status = ref<ServiceStatus>({ opencode: false, codex: false })
  const refreshing = ref(false)
  const launching = ref<ServiceId | null>(null)

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
      models.value = await invoke<string[]>('list_hive_models', { key: key.value.trim() })
      const first = models.value[0]
      if (first && !models.value.includes(selectedModel.value)) {
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

  async function saveKey() {
    const trimmed = key.value.trim()
    if (!trimmed || saving.value) return
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
    } catch (error) {
      toast.add({
        title: 'Salvataggio non riuscito',
        description: String(error),
        color: 'error'
      })
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
        description: 'Il processo è stato lanciato con la configurazione AI Hive.',
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
    refreshStatus,
    loadSavedKey,
    saveKey,
    clearKey,
    launch
  }
}
