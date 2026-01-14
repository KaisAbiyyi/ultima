<script lang="ts">
    // Bouncing dots typing indicator
</script>

<div class="typing-indicator text-gray-500 dark:text-gray-400">
    <span class="dot"></span>
    <span class="dot"></span>
    <span class="dot"></span>
</div>

<style>
    .typing-indicator {
        display: flex;
        align-items: center;
        gap: 4px;
        padding: 8px 12px;
    }

    .dot {
        width: 8px;
        height: 8px;
        background-color: currentColor;
        border-radius: 50%;
        opacity: 0.6;
        animation: bounce 1.4s infinite ease-in-out both;
    }

    .dot:nth-child(1) {
        animation-delay: -0.32s;
    }

    .dot:nth-child(2) {
        animation-delay: -0.16s;
    }

    .dot:nth-child(3) {
        animation-delay: 0s;
    }

    @keyframes bounce {
        0%,
        80%,
        100% {
            transform: scale(0);
        }
        40% {
            transform: scale(1);
        }
    }
</style>
