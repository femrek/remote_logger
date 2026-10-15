package dev.femrek.flutter_remote_logging

import android.content.Context
import android.os.Build
import android.os.Handler
import android.os.Looper
import android.security.keystore.KeyGenParameterSpec
import android.security.keystore.KeyProperties
import android.security.keystore.StrongBoxUnavailableException
import android.util.Base64
import io.flutter.embedding.engine.plugins.FlutterPlugin
import io.flutter.plugin.common.MethodCall
import io.flutter.plugin.common.MethodChannel
import io.flutter.plugin.common.MethodChannel.MethodCallHandler
import io.flutter.plugin.common.MethodChannel.Result
import java.security.KeyPairGenerator
import java.security.KeyStore
import java.security.PrivateKey
import java.security.Signature
import java.security.spec.ECGenParameterSpec
import java.util.concurrent.Executors

class KeyAttestationPlugin : FlutterPlugin, MethodCallHandler {
    private lateinit var channel: MethodChannel
    private var context: Context? = null
    private val executor = Executors.newSingleThreadExecutor()
    private val mainHandler = Handler(Looper.getMainLooper())

    companion object {
        private const val CHANNEL_NAME = "dev.femrek.flutter_remote_logging/attestation"
        private const val KEY_ALIAS = "frl_attest_key_v1"
        private const val PREFS_NAME = "frl_attestation_prefs"
        private const val PREF_KEY_TOKEN = "enrollment_token"
    }

    override fun onAttachedToEngine(flutterPluginBinding: FlutterPlugin.FlutterPluginBinding) {
        context = flutterPluginBinding.applicationContext
        channel = MethodChannel(flutterPluginBinding.binaryMessenger, CHANNEL_NAME)
        channel.setMethodCallHandler(this)
    }

    override fun onDetachedFromEngine(binding: FlutterPlugin.FlutterPluginBinding) {
        channel.setMethodCallHandler(null)
        context = null
    }

    override fun onMethodCall(call: MethodCall, result: Result) {
        when (call.method) {
            "isSupported" -> {
                result.success(Build.VERSION.SDK_INT >= Build.VERSION_CODES.N)
            }
            "enroll" -> {
                if (Build.VERSION.SDK_INT < Build.VERSION_CODES.N) {
                    result.error("UNSUPPORTED", "Key Attestation requires Android 7.0 (API 24) or higher", null)
                    return
                }
                val challenge = call.argument<String>("challenge")
                if (challenge.isNullOrEmpty()) {
                    result.error("INVALID_ARGUMENTS", "challenge is required", null)
                    return
                }

                executor.execute {
                    try {
                        val enrollData = generateAndAttestKey(challenge)
                        mainHandler.post { result.success(enrollData) }
                    } catch (e: Exception) {
                        mainHandler.post {
                            result.error("KEY_ATTEST_FAILED", e.message ?: "Key attestation failed", null)
                        }
                    }
                }
            }
            "prove" -> {
                val challenge = call.argument<String>("challenge")
                if (challenge.isNullOrEmpty()) {
                    result.error("INVALID_ARGUMENTS", "challenge is required", null)
                    return
                }

                executor.execute {
                    try {
                        val signatureB64 = signChallenge(challenge)
                        mainHandler.post { result.success(signatureB64) }
                    } catch (e: Exception) {
                        mainHandler.post {
                            result.error("PROVE_FAILED", e.message ?: "Failed to sign challenge", null)
                        }
                    }
                }
            }
            "loadEnrollmentToken" -> {
                val prefs = context?.getSharedPreferences(PREFS_NAME, Context.MODE_PRIVATE)
                val token = prefs?.getString(PREF_KEY_TOKEN, null)
                result.success(token)
            }
            "saveEnrollmentToken" -> {
                val token = call.argument<String>("token")
                val prefs = context?.getSharedPreferences(PREFS_NAME, Context.MODE_PRIVATE)
                prefs?.edit()?.putString(PREF_KEY_TOKEN, token)?.apply()
                result.success(null)
            }
            "reset" -> {
                executor.execute {
                    try {
                        val ks = KeyStore.getInstance("AndroidKeyStore")
                        ks.load(null)
                        if (ks.containsAlias(KEY_ALIAS)) {
                            ks.deleteEntry(KEY_ALIAS)
                        }
                    } catch (_: Exception) {
                    }
                    val prefs = context?.getSharedPreferences(PREFS_NAME, Context.MODE_PRIVATE)
                    prefs?.edit()?.remove(PREF_KEY_TOKEN)?.apply()
                    mainHandler.post { result.success(null) }
                }
            }
            else -> result.notImplemented()
        }
    }

    private fun generateAndAttestKey(challenge: String): Map<String, String> {
        val ks = KeyStore.getInstance("AndroidKeyStore")
        ks.load(null)
        if (ks.containsAlias(KEY_ALIAS)) {
            ks.deleteEntry(KEY_ALIAS)
        }

        var keyPair = tryGenerateKey(challenge, useStrongBox = true)
        if (keyPair == null) {
            keyPair = tryGenerateKey(challenge, useStrongBox = false)
        }

        if (keyPair == null) {
            throw IllegalStateException("Failed to generate hardware key pair in TEE or StrongBox")
        }

        ks.load(null)
        val chain = ks.getCertificateChain(KEY_ALIAS)
            ?: throw IllegalStateException("Certificate chain missing after key generation")

        // Concatenate certificate chain in PEM format
        val pemBuilder = StringBuilder()
        for (cert in chain) {
            pemBuilder.append("-----BEGIN CERTIFICATE-----\n")
            pemBuilder.append(Base64.encodeToString(cert.encoded, Base64.NO_WRAP))
            pemBuilder.append("\n-----END CERTIFICATE-----\n")
        }

        val pubKeyB64 = Base64.encodeToString(chain[0].publicKey.encoded, Base64.NO_WRAP)
        val certChainB64 = Base64.encodeToString(pemBuilder.toString().toByteArray(Charsets.UTF_8), Base64.NO_WRAP)

        return mapOf(
            "publicKey" to pubKeyB64,
            "attestation" to certChainB64
        )
    }

    private fun tryGenerateKey(challenge: String, useStrongBox: Boolean): java.security.KeyPair? {
        if (Build.VERSION.SDK_INT < Build.VERSION_CODES.N) return null

        val kpg = KeyPairGenerator.getInstance(KeyProperties.KEY_ALGORITHM_EC, "AndroidKeyStore")
        val builder = KeyGenParameterSpec.Builder(
            KEY_ALIAS,
            KeyProperties.PURPOSE_SIGN
        )
            .setAlgorithmParameterSpec(ECGenParameterSpec("secp256r1"))
            .setDigests(KeyProperties.DIGEST_SHA256)
            .setAttestationChallenge(challenge.toByteArray(Charsets.UTF_8))

        if (useStrongBox && Build.VERSION.SDK_INT >= Build.VERSION_CODES.P) {
            builder.setIsStrongBoxBacked(true)
        }

        return try {
            kpg.initialize(builder.build())
            kpg.generateKeyPair()
        } catch (e: Exception) {
            if (useStrongBox && (e is StrongBoxUnavailableException || e.javaClass.simpleName.contains("StrongBox"))) {
                null
            } else if (!useStrongBox) {
                throw e
            } else {
                null
            }
        }
    }

    private fun signChallenge(challenge: String): String {
        val ks = KeyStore.getInstance("AndroidKeyStore")
        ks.load(null)
        val privateKey = ks.getKey(KEY_ALIAS, null) as? PrivateKey
            ?: throw IllegalStateException("Key alias $KEY_ALIAS not found in Keystore")

        val signature = Signature.getInstance("SHA256withECDSA")
        signature.initSign(privateKey)
        signature.update(challenge.toByteArray(Charsets.UTF_8))
        val sigBytes = signature.sign()
        return Base64.encodeToString(sigBytes, Base64.NO_WRAP)
    }
}
