<script setup>
useHead({
  meta: [
    { name: 'viewport', content: 'width=device-width, initial-scale=1' }
  ],
  link: [
    { rel: 'icon', href: '/favicon.ico' }
  ],
  htmlAttrs: {
    lang: 'it'
  }
})

const title = 'ReQurv Launch'
const description = 'Lancia i tuoi agenti AI già configurati per AI Hive, l\'AI Gateway di ReQurv.'

useSeoMeta({
  title,
  description,
  ogTitle: title,
  ogDescription: description,
  twitterCard: 'summary_large_image'
})
</script>

<template>
  <UApp>
    <UHeader>
      <template #left>
        <NuxtLink
          to="/"
          class="focus-visible:outline-3 outline-primary/25 rounded-md p-1 -ms-1"
        >
          <AppLogo class="w-auto h-6 shrink-0" />
        </NuxtLink>

        <UButton
          to="https://hive.requrv.ai"
          target="_blank"
          label="AI Hive"
          icon="i-lucide-external-link"
          color="neutral"
          variant="ghost"
          size="sm"
        />
      </template>

      <template #right>
        <UColorModeButton />
      </template>
    </UHeader>

    <UMain>
      <NuxtPage />
    </UMain>

    <USeparator icon="i-lucide-hexagon" />

    <UFooter>
      <template #left>
        <p class="text-sm text-muted">
          ReQurv Launch © {{ new Date().getFullYear() }} — powered by
          <a
            href="https://hive.requrv.ai"
            target="_blank"
            class="text-primary hover:underline"
          >AI Hive</a>
        </p>
      </template>
    </UFooter>
  </UApp>
</template>
