<script setup lang="ts">
const hive = useHive()
const { restartModalOpen, restarting } = hive
</script>

<template>
  <UModal
    :open="restartModalOpen"
    title="ChatGPT è in esecuzione"
    description="Il catalogo modelli viene letto all'avvio dell'app."
    :ui="{ content: 'max-w-md' }"
    @update:open="restartModalOpen = $event"
  >
    <template #body>
      <p class="text-sm text-muted">
        Riavviare ChatGPT per applicare i cambiamenti? Una sessione in corso verrà chiusa.
      </p>
    </template>

    <template #footer>
      <div class="flex items-center justify-end gap-3">
        <UButton
          label="Più tardi"
          color="neutral"
          variant="ghost"
          @click="hive.cancelChatgptRestart()"
        />
        <UButton
          icon="i-lucide-rotate-cw"
          label="Riavvia e apri"
          :loading="restarting"
          @click="hive.confirmChatgptRestart()"
        />
      </div>
    </template>
  </UModal>
</template>
