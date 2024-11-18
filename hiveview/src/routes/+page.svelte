<script>
    import { onMount } from "svelte";

    // Sample API endpoints for demonstration purposes
    let status = "Loading...";
    let responseData = null;
    let jobInput = "";

    // Fetch Hive/Brain status on load
    async function fetchStatus() {
        try {
            const res = await fetch("/api/status");
            if (res.ok) {
                const data = await res.json();
                status = `Hive Status: ${data.hiveStatus} | Brain Status: ${data.brainStatus}`;
            } else {
                status = "Error fetching status.";
            }
        } catch (error) {
            status = "Failed to connect to backend.";
        }
    }

    // Send a job request to the backend
    async function startJob() {
        try {
            const res = await fetch("/api/start-job", {
                method: "POST",
                headers: { "Content-Type": "application/json" },
                body: JSON.stringify({ job: jobInput }),
            });
            if (res.ok) {
                const data = await res.json();
                responseData = `Job started: ${data.jobId}`;
            } else {
                responseData = "Failed to start job.";
            }
        } catch (error) {
            responseData = "Error communicating with backend.";
        }
    }

    onMount(fetchStatus);
</script>

<h1>Hive Control Panel</h1>

<div class="container">
    <!-- Status Section -->
    <div class="status">
        {status}
    </div>

    <!-- Job Invocation Form -->
    <div class="form">
        <h2>Start a New Job</h2>
        <div class="form-group">
            <input
                type="text"
                placeholder="Enter job name or parameters..."
                bind:value={jobInput}
            />
            <button on:click={startJob}>Start Job</button>
        </div>
    </div>

    <!-- Response Display -->
    {#if responseData}
        <div class="response">
            {responseData}
        </div>
    {/if}
</div>

<style>
    :global(body) {
        font-family: 'Inter', sans-serif;
        margin: 0;
        padding: 0;
        background-color: #1e1e1e;
        color: #f6f6f6;
        line-height: 1.6;
    }

    h1 {
        text-align: center;
        margin-top: 2rem;
        font-size: 2.5rem;
        color: #24c8db;
    }

    .container {
        max-width: 700px;
        margin: 3rem auto;
        padding: 2rem;
        background: #2e2e2e;
        border-radius: 12px;
        box-shadow: 0 8px 16px rgba(0, 0, 0, 0.2);
    }

    .status {
        font-size: 1.2rem;
        margin-bottom: 2rem;
        padding: 1rem;
        background: #333;
        border-radius: 8px;
        text-align: center;
        box-shadow: inset 0 2px 4px rgba(0, 0, 0, 0.2);
    }

    .form {
        margin-top: 2rem;
    }

    .form h2 {
        margin-bottom: 1rem;
        font-size: 1.5rem;
        color: #24c8db;
    }

    .form-group {
        display: flex;
        flex-direction: column;
        gap: 1rem;
    }

    input {
        padding: 0.75rem;
        font-size: 1rem;
        border: 1px solid #555;
        border-radius: 6px;
        background: #444;
        color: #f6f6f6;
        transition: border-color 0.3s;
    }

    input:focus {
        outline: none;
        border-color: #24c8db;
    }

    button {
        padding: 0.75rem;
        font-size: 1rem;
        font-weight: bold;
        text-transform: uppercase;
        color: #fff;
        background-color: #24c8db;
        border: none;
        border-radius: 6px;
        cursor: pointer;
        transition: background-color 0.3s, transform 0.2s;
    }

    button:hover {
        background-color: #1ba6b9;
        transform: scale(1.05);
    }

    .response {
        margin-top: 2rem;
        padding: 1rem;
        background: #333;
        border-radius: 8px;
        box-shadow: inset 0 2px 4px rgba(0, 0, 0, 0.2);
    }
</style>
