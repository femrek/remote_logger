Pod::Spec.new do |s|
  s.name             = 'flutter_remote_logging'
  s.version          = '0.0.2'
  s.summary          = 'Hardware Attestation for flutter_remote_logging'
  s.description      = <<-DESC
App Attest hardware attestation provider for flutter_remote_logging.
                       DESC
  s.homepage         = 'https://github.com/femrek/remote_logger'
  s.license          = { :type => 'MIT', :file => '../LICENSE' }
  s.author           = { 'femrek' => 'femrek@example.com' }
  s.source           = { :path => '.' }
  s.source_files = 'Classes/**/*'
  s.dependency 'Flutter'
  s.platform = :ios, '14.0'

  # Flutter.framework does not contain a i386 slice.
  s.pod_target_xcconfig = { 'DEFINES_MODULE' => 'YES', 'EXCLUDED_ARCHS[sdk=iphonesimulator*]' => 'i386' }
  s.swift_version = '5.0'
end
