using System.IO;
using System.Net;
using System.Net.Http;
using System.Net.Http.Headers;
using System.Net.Http.Json;
using System.Security.Cryptography;
using System.Text;
using System.Text.Json;
using System.Text.Json.Serialization;

namespace AzureTcoCalculator.Companion.Api;

public sealed class CalculatorLaunchClient : IDisposable
{
    private const int MaximumResponseBytes = 300 * 1024;
    private static readonly JsonSerializerOptions JsonOptions = new()
    {
        PropertyNameCaseInsensitive = false,
        UnmappedMemberHandling = JsonUnmappedMemberHandling.Disallow,
        MaxDepth = 16
    };

    private readonly HttpClient _client;

    public CalculatorLaunchClient(Uri apiBaseUri)
    {
        _client = new HttpClient(new SocketsHttpHandler { AllowAutoRedirect = false })
        {
            BaseAddress = apiBaseUri,
            Timeout = TimeSpan.FromSeconds(20)
        };
        _client.DefaultRequestHeaders.Accept.Add(new MediaTypeWithQualityHeaderValue("application/json"));
    }

    public async Task<ClaimedLaunch> ClaimAsync(
        Guid launchId,
        Guid companionInstanceId,
        string accessToken,
        CancellationToken cancellationToken)
    {
        int[] delays = [100, 200, 400, 800, 1_200, 1_600, 1_800, 1_900];
        for (int attempt = 0; ; attempt++)
        {
            using HttpRequestMessage request = new(
                HttpMethod.Post,
                $"calculator-launches/{launchId:D}/claim")
            {
                Content = JsonContent.Create(new ClaimRequest(companionInstanceId))
            };
            request.Headers.Authorization = new AuthenticationHeaderValue("Bearer", accessToken);
            using HttpResponseMessage response = await _client.SendAsync(
                request,
                HttpCompletionOption.ResponseHeadersRead,
                cancellationToken).ConfigureAwait(false);
            if (response.StatusCode == HttpStatusCode.NotFound && attempt < delays.Length)
            {
                await Task.Delay(delays[attempt], cancellationToken).ConfigureAwait(false);
                continue;
            }
            if (response.StatusCode != HttpStatusCode.OK)
            {
                throw await CreateApiExceptionAsync(response, cancellationToken).ConfigureAwait(false);
            }
            if (response.Content.Headers.ContentType?.MediaType != "application/json")
            {
                throw new InvalidDataException("The claim response content type is invalid.");
            }
            string etag = response.Headers.ETag?.ToString()
                ?? throw new InvalidDataException("The claim response did not include an ETag.");
            if (etag.Length is < 3 or > 128 || etag[0] != '"' || etag[^1] != '"')
            {
                throw new InvalidDataException("The claim response ETag is invalid.");
            }
            byte[] body = await ReadBoundedAsync(response, cancellationToken).ConfigureAwait(false);
            ClaimEnvelope envelope = JsonSerializer.Deserialize<ClaimEnvelope>(body, JsonOptions)
                ?? throw new InvalidDataException("The claim response was empty.");
            string rawManifest = envelope.Manifest.GetRawText();
            string actualHash = Convert.ToHexStringLower(SHA256.HashData(Encoding.UTF8.GetBytes(rawManifest)));
            if (envelope.ManifestSha256.Length != 64
                || !CryptographicOperations.FixedTimeEquals(
                    Encoding.ASCII.GetBytes(actualHash),
                    Encoding.ASCII.GetBytes(envelope.ManifestSha256)))
            {
                throw new InvalidDataException("The Calculator manifest integrity check failed.");
            }
            CalculatorManifest manifest = envelope.Manifest.Deserialize<CalculatorManifest>(JsonOptions)
                ?? throw new InvalidDataException("The Calculator manifest was empty.");
            manifest.Validate();
            return new ClaimedLaunch(etag, manifest);
        }
    }

    public async Task AcknowledgeAsync(
        Guid launchId,
        Guid companionInstanceId,
        string etag,
        string accessToken,
        CancellationToken cancellationToken)
    {
        using HttpRequestMessage request = new(
            HttpMethod.Post,
            $"calculator-launches/{launchId:D}/acknowledge")
        {
            Content = JsonContent.Create(new AcknowledgeRequest(companionInstanceId))
        };
        request.Headers.Authorization = new AuthenticationHeaderValue("Bearer", accessToken);
        request.Headers.TryAddWithoutValidation("If-Match", etag);
        using HttpResponseMessage response = await _client.SendAsync(request, cancellationToken).ConfigureAwait(false);
        if (response.StatusCode != HttpStatusCode.NoContent)
        {
            throw await CreateApiExceptionAsync(response, cancellationToken).ConfigureAwait(false);
        }
    }

    public void Dispose() => _client.Dispose();

    private static async Task<byte[]> ReadBoundedAsync(
        HttpResponseMessage response,
        CancellationToken cancellationToken)
    {
        if (response.Content.Headers.ContentLength > MaximumResponseBytes)
        {
            throw new InvalidDataException("The API response exceeded the allowed size.");
        }
        await using Stream input = await response.Content.ReadAsStreamAsync(cancellationToken).ConfigureAwait(false);
        using MemoryStream output = new();
        byte[] buffer = new byte[16 * 1024];
        while (true)
        {
            int read = await input.ReadAsync(buffer, cancellationToken).ConfigureAwait(false);
            if (read == 0)
            {
                return output.ToArray();
            }
            if (output.Length + read > MaximumResponseBytes)
            {
                throw new InvalidDataException("The API response exceeded the allowed size.");
            }
            output.Write(buffer, 0, read);
        }
    }

    private static async Task<InvalidOperationException> CreateApiExceptionAsync(
        HttpResponseMessage response,
        CancellationToken cancellationToken)
    {
        byte[] body = await ReadBoundedAsync(response, cancellationToken).ConfigureAwait(false);
        try
        {
            ApiProblem? problem = JsonSerializer.Deserialize<ApiProblem>(body, JsonOptions);
            if (!string.IsNullOrWhiteSpace(problem?.Detail))
            {
                return new InvalidOperationException(problem.Detail);
            }
        }
        catch (JsonException)
        {
        }
        return new InvalidOperationException($"The secure handoff failed with status {(int)response.StatusCode}.");
    }

    private sealed record ClaimRequest(
        [property: JsonPropertyName("companion_instance_id")] Guid CompanionInstanceId,
        [property: JsonPropertyName("companion_version")] string CompanionVersion = "1.0.0",
        [property: JsonPropertyName("supported_protocol_versions")] int[]? ProtocolVersions = null,
        [property: JsonPropertyName("supported_manifest_versions")] int[]? ManifestVersions = null,
        [property: JsonPropertyName("supported_calculator_contracts")] string[]? CalculatorContracts = null)
    {
        public ClaimRequest(Guid companionInstanceId)
            : this(companionInstanceId, "1.0.0", [1], [2], ["2026-08-24"])
        {
        }
    }

    private sealed record AcknowledgeRequest(
        [property: JsonPropertyName("companion_instance_id")] Guid CompanionInstanceId);

    private sealed record ClaimEnvelope(
        [property: JsonPropertyName("manifest_sha256")] string ManifestSha256,
        [property: JsonPropertyName("manifest")] JsonElement Manifest);

    private sealed record ApiProblem([property: JsonPropertyName("detail")] string Detail);
}

public sealed record ClaimedLaunch(string ETag, CalculatorManifest Manifest);