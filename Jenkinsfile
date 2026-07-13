pipeline {
    agent any

    environment {
        // Automatically mapped to ghcr.io based on docker-compose.yaml configuration
        DOCKER_REGISTRY = 'ghcr.io/femrek' 
        
        // env.TAG_NAME is automatically populated by Jenkins when a tag is built
        IMAGE_TAG = "${env.TAG_NAME}" 
        
        // Enable Docker BuildKit for advanced Dockerfile features like --mount
        DOCKER_BUILDKIT = '1'

        // Suppress docker-compose warnings during validation and teardown
        OPENSEARCH_ADMIN_PASSWORD = 'StrongPassword123!'
        GATEWAY_JWT_SECRET = 'ci_default_secret_for_e2e_testing_32_chars_long'
    }

    stages {
        stage('CI Pipeline') {
            agent {
                dockerfile {
                    filename 'Dockerfile.ci'
                    // Run as root inside the container to guarantee permissions to access /var/run/docker.sock
                    args '-u root -v /var/run/docker.sock:/var/run/docker.sock --network host'
                }
            }
            environment {
                CARGO_HOME = "${WORKSPACE}/.cargo"
            }
            stages {
                stage('Test logger_gateway') {
                    steps {
                        script {
                            echo 'Running logger_gateway unit tests...'
                            sh 'cd logger_gateway && cargo test'
                        }
                    }
                }

                stage('Test validation_worker') {
                    steps {
                        script {
                            echo 'Running validation_worker unit and integration tests...'
                            sh 'cd validation_worker && cargo test'
                        }
                    }
                }

                stage('Test E2E Pipeline') {
                    steps {
                        script {
                            echo 'Running End-to-End full stack integration tests...'
                            sh 'cd e2e && cargo test'
                        }
                    }
                }
            }
            post {
                always {
                    script {
                        echo 'Cleaning up any leftover docker-compose containers or volumes...'
                        sh 'docker compose -f docker/docker-compose.yaml -f docker/docker-compose.dev.yaml down -v --remove-orphans || true'
                    }
                }
            }
        }

        stage('Verify Tag') {
            when {
                buildingTag()
            }
            steps {
                script {
                    echo "Building Docker images for tag: ${env.TAG_NAME}"
                }
            }
        }

        stage('Build Docker Images') {
            when {
                buildingTag()
            }
            steps {
                script {
                    // Build logger_gateway from its context and Dockerfile
                    echo "Building logger_gateway image..."
                    gatewayImage = docker.build("${DOCKER_REGISTRY}/logger_gateway:${IMAGE_TAG}", "-f logger_gateway/docker/Dockerfile logger_gateway")
                    
                    // Build validation_worker from its context and Dockerfile
                    echo "Building validation_worker image..."
                    workerImage = docker.build("${DOCKER_REGISTRY}/validation_worker:${IMAGE_TAG}", "-f validation_worker/Dockerfile validation_worker")
                }
            }
        }

        stage('Push Docker Images') {
            when {
                buildingTag()
            }
            steps {
                script {
                    // Provide the Jenkins credential ID 'github-credentials' matching your GHCR config
                    docker.withRegistry('https://ghcr.io', 'github-general-pat') {
                        gatewayImage.push()
                        gatewayImage.push('latest')
                        
                        workerImage.push()
                        workerImage.push('latest') 
                    }
                }
            }
        }
    }
    
    post {
        always {
            script {
                // Only clean up images if a tag was built and images were generated
                if (env.TAG_NAME) {
                    sh "docker rmi ${env.DOCKER_REGISTRY}/logger_gateway:${env.IMAGE_TAG} || true"
                    sh "docker rmi ${env.DOCKER_REGISTRY}/validation_worker:${env.IMAGE_TAG} || true"
                }
            }
        }
    }
}
